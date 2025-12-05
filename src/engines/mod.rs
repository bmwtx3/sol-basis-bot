//! Calculation Engines Module - Phase 3
//!
//! Provides real-time calculation engines for:
//! - Funding rate analysis and prediction
//! - Basis spread calculation and hedge ratios
//! - Trade signal generation

pub mod funding_engine;
pub mod basis_engine;
pub mod signal_engine;

pub use funding_engine::FundingEngine;
pub use basis_engine::BasisEngine;
pub use signal_engine::SignalEngine;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use crate::config::AppConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;

/// Engine manager that coordinates all calculation engines
pub struct EngineManager {
    /// Funding engine
    pub funding: FundingEngine,
    /// Basis engine
    pub basis: BasisEngine,
    /// Signal engine
    pub signal: SignalEngine,
}

impl EngineManager {
    /// Create a new engine manager
    pub fn new(
        config: Arc<AppConfig>,
        state: Arc<SharedState>,
        event_tx: broadcast::Sender<Event>,
    ) -> Self {
        Self {
            funding: FundingEngine::new(config.clone(), state.clone(), event_tx.clone()),
            basis: BasisEngine::new(config.clone(), state.clone(), event_tx.clone()),
            signal: SignalEngine::new(config, state, event_tx),
        }
    }
    
    /// Start all engines
    pub async fn start(&self) -> Result<()> {
        info!("Starting calculation engines...");
        
        self.funding.start().await?;
        self.basis.start().await?;
        self.signal.start().await?;
        
        info!("All calculation engines started");
        Ok(())
    }
    
    /// Stop all engines
    pub async fn stop(&self) {
        info!("Stopping calculation engines...");
        self.funding.stop().await;
        self.basis.stop().await;
        self.signal.stop().await;
    }
}
