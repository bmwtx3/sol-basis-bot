//! Price Feeds Module - Phase 2
//!
//! Provides real-time price feeds from multiple sources:
//! - Pyth oracle for SOL/USD
//! - Jupiter for spot aggregation
//! - Drift Protocol for perp prices

pub mod pyth;
pub mod jupiter;
pub mod drift;

pub use pyth::PythFeed;
pub use jupiter::JupiterFeed;
pub use drift::DriftFeed;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use crate::config::ProtocolsConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;

/// Price feed manager that coordinates all price sources
pub struct PriceFeedManager {
    /// Pyth feed
    pub pyth: PythFeed,
    /// Jupiter feed
    pub jupiter: JupiterFeed,
    /// Drift feed
    pub drift: DriftFeed,
    /// Shared state
    state: Arc<SharedState>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
}

impl PriceFeedManager {
    /// Create a new price feed manager
    pub fn new(
        config: &ProtocolsConfig,
        state: Arc<SharedState>,
        event_tx: broadcast::Sender<Event>,
    ) -> Self {
        Self {
            pyth: PythFeed::new(&config.pyth, event_tx.clone()),
            jupiter: JupiterFeed::new(&config.jupiter, event_tx.clone()),
            drift: DriftFeed::new(&config.drift, event_tx.clone()),
            state,
            event_tx,
        }
    }
    
    /// Start all price feeds
    pub async fn start(&self) -> Result<()> {
        info!("Starting price feed manager");
        
        // Start individual feeds
        self.pyth.start().await?;
        self.jupiter.start().await?;
        self.drift.start().await?;
        
        info!("All price feeds started");
        Ok(())
    }
    
    /// Stop all price feeds
    pub async fn stop(&self) {
        info!("Stopping price feed manager");
        self.pyth.stop().await;
        self.jupiter.stop().await;
        self.drift.stop().await;
    }
    
    /// Get current spot price (best available)
    pub fn get_spot_price(&self) -> f64 {
        self.state.spot_price.load()
    }
    
    /// Get current perp mark price
    pub fn get_perp_mark_price(&self) -> f64 {
        self.state.perp_mark_price.load()
    }
    
    /// Get current basis spread
    pub fn get_basis_spread(&self) -> f64 {
        self.state.get_basis_spread()
    }
}
