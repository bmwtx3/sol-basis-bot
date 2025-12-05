//! Event Bus for Internal Communication
//!
//! Provides a broadcast-based event system for decoupled communication
//! between modules, particularly for price updates.

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::utils::types::PriceUpdate;

/// Event types that can be broadcast through the system
#[derive(Debug, Clone)]
pub enum Event {
    // Connection events
    WebSocketConnected,
    WebSocketDisconnected,
    WebSocketMessage(String),
    
    // Price events
    SpotPriceUpdate(PriceUpdate),
    PerpMarkPriceUpdate(PriceUpdate),
    PerpIndexPriceUpdate(PriceUpdate),
    
    // Funding events
    FundingRateUpdate {
        rate: f64,
        timestamp: i64,
    },
    
    // Basis events
    BasisSpreadUpdate {
        spread: f64,
        spot_price: f64,
        perp_price: f64,
        timestamp: i64,
    },
    
    // Trading signals
    TradeSignal {
        signal_type: String,
        size: f64,
        reason: String,
    },
    
    // System events
    SystemPause {
        reason: String,
    },
    SystemResume,
    Error {
        source: String,
        message: String,
    },
    
    // Position events
    PositionOpened {
        position_id: String,
        position_type: String,
        size: f64,
        price: f64,
    },
    PositionClosed {
        position_id: String,
        pnl: f64,
    },
    
    // Heartbeat
    Heartbeat {
        timestamp: i64,
    },
}

/// Event bus for broadcasting events to multiple subscribers
pub struct EventBus {
    /// Broadcast sender
    sender: broadcast::Sender<Event>,
    /// Channel capacity
    capacity: usize,
}

impl EventBus {
    /// Create a new event bus with the given capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender, capacity }
    }
    
    /// Create with default capacity (1024)
    pub fn default() -> Self {
        Self::new(1024)
    }
    
    /// Get a sender for publishing events
    pub fn sender(&self) -> broadcast::Sender<Event> {
        self.sender.clone()
    }
    
    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
    
    /// Publish an event
    pub fn publish(&self, event: Event) {
        match self.sender.send(event) {
            Ok(count) => {
                debug!("Event sent to {} receivers", count);
            }
            Err(_) => {
                // No receivers - this is fine during startup/shutdown
                debug!("No event receivers");
            }
        }
    }
    
    /// Get number of active receivers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
    
    /// Get channel capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Event processor that handles events from the bus
pub struct EventProcessor {
    /// Event receiver
    receiver: broadcast::Receiver<Event>,
    /// Name for logging
    name: String,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new(bus: &EventBus, name: &str) -> Self {
        Self {
            receiver: bus.subscribe(),
            name: name.to_string(),
        }
    }
    
    /// Process next event (blocking)
    pub async fn next(&mut self) -> Option<Event> {
        match self.receiver.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(count)) => {
                warn!(
                    "Event processor '{}' lagged by {} messages",
                    self.name, count
                );
                // Try to get the next event
                self.receiver.recv().await.ok()
            }
            Err(broadcast::error::RecvError::Closed) => {
                debug!("Event bus closed for processor '{}'", self.name);
                None
            }
        }
    }
    
    /// Try to receive event without blocking
    pub fn try_next(&mut self) -> Option<Event> {
        match self.receiver.try_recv() {
            Ok(event) => Some(event),
            Err(_) => None,
        }
    }
}

/// Helper to create typed event handlers
pub fn spawn_event_handler<F, Fut>(
    bus: &EventBus,
    name: &str,
    mut handler: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnMut(Event) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let mut processor = EventProcessor::new(bus, name);
    let name = name.to_string();
    
    tokio::spawn(async move {
        debug!("Event handler '{}' started", name);
        while let Some(event) = processor.next().await {
            handler(event).await;
        }
        debug!("Event handler '{}' stopped", name);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_pubsub() {
        let bus = EventBus::new(10);
        let mut receiver = bus.subscribe();
        
        bus.publish(Event::Heartbeat {
            timestamp: 12345,
        });
        
        let event = receiver.recv().await.unwrap();
        match event {
            Event::Heartbeat { timestamp } => {
                assert_eq!(timestamp, 12345);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(10);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        
        bus.publish(Event::SystemResume);
        
        assert!(matches!(rx1.recv().await.unwrap(), Event::SystemResume));
        assert!(matches!(rx2.recv().await.unwrap(), Event::SystemResume));
    }
}
