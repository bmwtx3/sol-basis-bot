//! Execution Module - Phase 4
//!
//! Provides transaction execution infrastructure:
//! - Transaction builder for Drift + Jupiter
//! - Jito bundle integration for MEV protection
//! - Priority fee management
//! - Simulation and retry logic

pub mod tx_builder;
pub mod jupiter;
pub mod jito;
pub mod simulator;
pub mod submitter;

pub use tx_builder::TransactionBuilder;
pub use jupiter::JupiterClient;
pub use jito::JitoClient;
pub use simulator::TransactionSimulator;
pub use submitter::TransactionSubmitter;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::AppConfig;
use crate::network::RpcManager;
use crate::state::SharedState;

/// Execution manager coordinates all execution components
pub struct ExecutionManager {
    /// Transaction builder
    pub tx_builder: TransactionBuilder,
    /// Jupiter client for swaps
    pub jupiter: JupiterClient,
    /// Jito client for bundles
    pub jito: Option<JitoClient>,
    /// Transaction simulator
    pub simulator: TransactionSimulator,
    /// Transaction submitter
    pub submitter: TransactionSubmitter,
    /// Is execution enabled
    enabled: Arc<RwLock<bool>>,
}

impl ExecutionManager {
    /// Create a new execution manager
    pub async fn new(
        config: Arc<AppConfig>,
        rpc: Arc<RpcManager>,
        _state: Arc<SharedState>,
    ) -> Result<Self> {
        let tx_builder = TransactionBuilder::new(config.clone(), rpc.clone())?;
        let jupiter = JupiterClient::new(&config.protocols.jupiter)?;
        let simulator = TransactionSimulator::new(rpc.clone());
        let submitter = TransactionSubmitter::new(config.clone(), rpc.clone());
        
        // Initialize Jito if enabled
        let jito = if config.execution.use_jito {
            Some(JitoClient::new(&config.execution)?)
        } else {
            None
        };
        
        Ok(Self {
            tx_builder,
            jupiter,
            jito,
            simulator,
            submitter,
            enabled: Arc::new(RwLock::new(!config.paper_trading)),
        })
    }
    
    /// Check if execution is enabled
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }
    
    /// Enable execution
    pub async fn enable(&self) {
        *self.enabled.write().await = true;
        info!("Execution enabled");
    }
    
    /// Disable execution (paper trading mode)
    pub async fn disable(&self) {
        *self.enabled.write().await = false;
        info!("Execution disabled (paper trading)");
    }
    
    /// Check if Jito is available
    pub fn has_jito(&self) -> bool {
        self.jito.is_some()
    }
}
