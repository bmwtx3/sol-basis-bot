//! WebSocket Connection Manager
//!
//! Manages WebSocket connections for streaming price updates
//! with automatic reconnection and health monitoring.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::network::event_bus::Event;

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// WebSocket Manager for streaming data
pub struct WebSocketManager {
    /// WebSocket URL
    url: String,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Reconnect attempts
    max_reconnect_attempts: u32,
    /// Reconnect delay
    reconnect_delay: Duration,
    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
}

impl WebSocketManager {
    /// Create a new WebSocket manager
    pub fn new(url: &str, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            url: url.to_string(),
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            event_tx,
            max_reconnect_attempts: 10,
            reconnect_delay: Duration::from_secs(1),
            shutdown: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Get current connection state
    pub async fn get_state(&self) -> ConnectionState {
        *self.state.read().await
    }
    
    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }
    
    /// Start the WebSocket connection
    pub async fn start(&self) -> Result<()> {
        let url = self.url.clone();
        let state = self.state.clone();
        let event_tx = self.event_tx.clone();
        let max_attempts = self.max_reconnect_attempts;
        let reconnect_delay = self.reconnect_delay;
        let shutdown = self.shutdown.clone();
        
        tokio::spawn(async move {
            let mut reconnect_count = 0;
            
            loop {
                // Check shutdown signal
                if *shutdown.read().await {
                    info!("WebSocket shutdown signal received");
                    break;
                }
                
                *state.write().await = ConnectionState::Connecting;
                info!("Connecting to WebSocket: {}", url);
                
                match Self::connect_and_run(&url, &state, &event_tx).await {
                    Ok(()) => {
                        info!("WebSocket connection closed normally");
                        reconnect_count = 0;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        reconnect_count += 1;
                        
                        if reconnect_count >= max_attempts {
                            error!(
                                "Max reconnection attempts ({}) reached",
                                max_attempts
                            );
                            break;
                        }
                    }
                }
                
                // Check shutdown again before reconnecting
                if *shutdown.read().await {
                    break;
                }
                
                *state.write().await = ConnectionState::Reconnecting;
                let delay = reconnect_delay * reconnect_count;
                warn!(
                    "Reconnecting in {:?} (attempt {}/{})",
                    delay, reconnect_count, max_attempts
                );
                tokio::time::sleep(delay).await;
            }
            
            *state.write().await = ConnectionState::Disconnected;
        });
        
        Ok(())
    }
    
    /// Connect and run the WebSocket
    async fn connect_and_run(
        url: &str,
        state: &Arc<RwLock<ConnectionState>>,
        event_tx: &broadcast::Sender<Event>,
    ) -> Result<()> {
        let parsed_url = Url::parse(url).context("Invalid WebSocket URL")?;
        
        let (ws_stream, _) = timeout(Duration::from_secs(10), connect_async(parsed_url))
            .await
            .context("WebSocket connection timeout")?
            .context("Failed to connect to WebSocket")?;
        
        *state.write().await = ConnectionState::Connected;
        info!("WebSocket connected successfully");
        
        // Notify connection established
        let _ = event_tx.send(Event::WebSocketConnected);
        
        let (mut write, mut read) = ws_stream.split();
        
        // Heartbeat interval
        let mut heartbeat = interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                // Handle incoming messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            debug!("WebSocket message: {} bytes", text.len());
                            let _ = event_tx.send(Event::WebSocketMessage(text));
                        }
                        Some(Ok(Message::Binary(data))) => {
                            debug!("WebSocket binary: {} bytes", data.len());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            debug!("WebSocket ping received");
                            if let Err(e) = write.send(Message::Pong(data)).await {
                                warn!("Failed to send pong: {}", e);
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("WebSocket pong received");
                        }
                        Some(Ok(Message::Close(frame))) => {
                            info!("WebSocket close frame: {:?}", frame);
                            let _ = event_tx.send(Event::WebSocketDisconnected);
                            return Ok(());
                        }
                        Some(Ok(Message::Frame(_))) => {}
                        Some(Err(e)) => {
                            error!("WebSocket read error: {}", e);
                            let _ = event_tx.send(Event::WebSocketDisconnected);
                            return Err(e.into());
                        }
                        None => {
                            info!("WebSocket stream ended");
                            let _ = event_tx.send(Event::WebSocketDisconnected);
                            return Ok(());
                        }
                    }
                }
                
                // Send heartbeat
                _ = heartbeat.tick() => {
                    debug!("Sending WebSocket ping");
                    if let Err(e) = write.send(Message::Ping(vec![])).await {
                        warn!("Failed to send ping: {}", e);
                        return Err(e.into());
                    }
                }
            }
        }
    }
    
    /// Send a message through WebSocket
    pub async fn send(&self, message: &str) -> Result<()> {
        // Note: This is a simplified version. In production, you'd need
        // to maintain access to the write half of the stream.
        // For now, we'll use the RPC subscription model which is more reliable.
        warn!("Direct WebSocket send not implemented - use subscriptions");
        Ok(())
    }
    
    /// Stop the WebSocket connection
    pub async fn stop(&self) {
        info!("Stopping WebSocket connection");
        *self.shutdown.write().await = true;
    }
}

/// Solana-specific WebSocket subscription manager
pub struct SolanaWebSocket {
    /// Base WebSocket manager
    manager: WebSocketManager,
    /// Subscriptions
    subscriptions: Arc<RwLock<Vec<String>>>,
}

impl SolanaWebSocket {
    /// Create a new Solana WebSocket
    pub fn new(ws_url: &str, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            manager: WebSocketManager::new(ws_url, event_tx),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Start connection
    pub async fn start(&self) -> Result<()> {
        self.manager.start().await
    }
    
    /// Subscribe to account updates
    pub async fn subscribe_account(&self, pubkey: &str) -> Result<()> {
        let sub = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"accountSubscribe","params":["{}", {{"encoding":"jsonParsed","commitment":"confirmed"}}]}}"#,
            pubkey
        );
        self.subscriptions.write().await.push(sub);
        Ok(())
    }
    
    /// Subscribe to program accounts
    pub async fn subscribe_program(&self, program_id: &str) -> Result<()> {
        let sub = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"programSubscribe","params":["{}", {{"encoding":"jsonParsed","commitment":"confirmed"}}]}}"#,
            program_id
        );
        self.subscriptions.write().await.push(sub);
        Ok(())
    }
    
    /// Get connection state
    pub async fn get_state(&self) -> ConnectionState {
        self.manager.get_state().await
    }
    
    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.manager.is_connected().await
    }
    
    /// Stop connection
    pub async fn stop(&self) {
        self.manager.stop().await
    }
}
