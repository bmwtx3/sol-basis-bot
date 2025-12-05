//! Agent Module - Phase 5
//!
//! Provides the agentic trading logic:
//! - State machine for trade lifecycle
//! - Risk management and circuit breakers
//! - Position tracking and P&L
//! - Rebalancing logic

pub mod state_machine;
pub mod risk_manager;
pub mod rebalancer;

pub use state_machine::{AgentStateMachine, AgentState, StateTransition};
pub use risk_manager::RiskManager;
pub use rebalancer::Rebalancer;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error, debug};

use crate::config::AppConfig;
use crate::engines::EngineManager;
use crate::execution::ExecutionManager;
use crate::network::event_bus::Event;
use crate::position::PositionManager;
use crate::state::SharedState;

/// Trading agent that coordinates all components
pub struct TradingAgent {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// State machine
    state_machine: Arc<RwLock<AgentStateMachine>>,
    /// Risk manager
    risk_manager: Arc<RiskManager>,
    /// Rebalancer
    rebalancer: Arc<Rebalancer>,
    /// Position manager
    position_manager: Arc<PositionManager>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
}

impl TradingAgent {
    /// Create a new trading agent
    pub fn new(
        config: Arc<AppConfig>,
        state: Arc<SharedState>,
        position_manager: Arc<PositionManager>,
        event_tx: broadcast::Sender<Event>,
    ) -> Self {
        let state_machine = Arc::new(RwLock::new(AgentStateMachine::new()));
        let risk_manager = Arc::new(RiskManager::new(config.clone(), state.clone()));
        let rebalancer = Arc::new(Rebalancer::new(
            config.clone(),
            state.clone(),
            position_manager.clone(),
        ));
        
        Self {
            config,
            state,
            state_machine,
            risk_manager,
            rebalancer,
            position_manager,
            event_tx,
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start the trading agent
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Trading agent starting");
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let state_machine = self.state_machine.clone();
        let risk_manager = self.risk_manager.clone();
        let rebalancer = self.rebalancer.clone();
        let position_manager = self.position_manager.clone();
        let event_tx = self.event_tx.clone();
        
        // Main agent loop
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            
            while *running.read().await {
                interval.tick().await;
                
                // Check risk conditions first
                let risk_check = risk_manager.check_all().await;
                
                if risk_check.should_pause {
                    let mut sm = state_machine.write().await;
                    if sm.current_state() != AgentState::Paused {
                        warn!("Risk check triggered pause: {:?}", risk_check.reasons);
                        sm.transition_to(AgentState::Paused);
                        let _ = event_tx.send(Event::SystemPause {
                            reason: risk_check.reasons.join("; "),
                        });
                    }
                    continue;
                }
                
                // Get current state
                let current_state = state_machine.read().await.current_state();
                
                match current_state {
                    AgentState::Idle => {
                        // Check for trade opportunities
                        if let Some(signal) = Self::check_for_signals(&state, &config).await {
                            info!("Trade signal detected: {:?}", signal);
                            let mut sm = state_machine.write().await;
                            sm.transition_to(AgentState::Opening);
                        }
                    }
                    
                    AgentState::Opening => {
                        // Execute opening trade
                        // In paper mode, just simulate
                        if config.paper_trading {
                            debug!("Paper trading: simulating open");
                            position_manager.simulate_open(
                                state.spot_price.load(),
                                100.0, // Simulated size
                            ).await;
                        }
                        
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Monitoring);
                    }
                    
                    AgentState::Monitoring => {
                        // Check for close or rebalance conditions
                        let basis = state.get_basis_spread();
                        
                        // Check for close condition
                        if basis.abs() < config.trading.basis_close_threshold_pct {
                            info!("Basis converged to {:.4}%, closing position", basis);
                            let mut sm = state_machine.write().await;
                            sm.transition_to(AgentState::Closing);
                            continue;
                        }
                        
                        // Check for rebalance
                        if rebalancer.needs_rebalance().await {
                            info!("Hedge drift detected, rebalancing");
                            let mut sm = state_machine.write().await;
                            sm.transition_to(AgentState::Rebalancing);
                        }
                    }
                    
                    AgentState::Closing => {
                        // Execute closing trade
                        if config.paper_trading {
                            debug!("Paper trading: simulating close");
                            position_manager.simulate_close(state.spot_price.load()).await;
                        }
                        
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Idle);
                        
                        // Log P&L
                        let pnl = position_manager.get_realized_pnl().await;
                        info!("Position closed. Realized P&L: ${:.2}", pnl);
                    }
                    
                    AgentState::Rebalancing => {
                        // Execute rebalance
                        if let Err(e) = rebalancer.execute_rebalance().await {
                            error!("Rebalance failed: {}", e);
                        }
                        
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Monitoring);
                    }
                    
                    AgentState::Paused => {
                        // Check if we can resume
                        if risk_manager.can_resume().await {
                            info!("Risk conditions cleared, resuming");
                            let mut sm = state_machine.write().await;
                            sm.transition_to(AgentState::Idle);
                            let _ = event_tx.send(Event::SystemResume);
                        }
                    }
                    
                    AgentState::Error => {
                        // Wait for manual intervention or timeout
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Idle);
                    }
                }
            }
            
            info!("Trading agent stopped");
        });
        
        Ok(())
    }
    
    /// Check for trade signals
    async fn check_for_signals(state: &Arc<SharedState>, config: &Arc<AppConfig>) -> Option<String> {
        let basis = state.get_basis_spread();
        let funding_apr = state.funding_apr.load();
        
        // Check minimum thresholds
        if basis.abs() >= config.trading.min_basis_spread_pct 
            && funding_apr.abs() >= config.trading.min_funding_apr_pct 
        {
            // Check alignment
            let aligned = (basis > 0.0 && funding_apr > 0.0) || (basis < 0.0 && funding_apr < 0.0);
            if aligned {
                return Some(format!(
                    "Basis: {:.4}%, Funding APR: {:.2}%",
                    basis, funding_apr
                ));
            }
        }
        
        None
    }
    
    /// Stop the trading agent
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Trading agent stopping");
    }
    
    /// Get current state
    pub async fn current_state(&self) -> AgentState {
        self.state_machine.read().await.current_state()
    }
    
    /// Get risk manager
    pub fn risk_manager(&self) -> &Arc<RiskManager> {
        &self.risk_manager
    }
    
    /// Get position manager
    pub fn position_manager(&self) -> &Arc<PositionManager> {
        &self.position_manager
    }
    
    /// Force pause (emergency stop)
    pub async fn emergency_stop(&self) {
        warn!("Emergency stop triggered");
        let mut sm = self.state_machine.write().await;
        sm.transition_to(AgentState::Paused);
        let _ = self.event_tx.send(Event::SystemPause {
            reason: "Emergency stop".to_string(),
        });
    }
}
