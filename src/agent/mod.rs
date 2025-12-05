//! Agent Module - Phase 5 + Agentic Features
//!
//! Provides the agentic trading logic:
//! - State machine for trade lifecycle
//! - Risk management and circuit breakers
//! - Position tracking and P&L
//! - Rebalancing logic
//! - **NEW** Performance-based learning
//! - **NEW** Adaptive position sizing
//! - **NEW** Funding reversal detection

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
use crate::agentic::{
    PerformanceDb, TradeOutcome, PerformanceMetrics,
    AdaptiveSizer, SizingRecommendation,
    ReversalDetector, ReversalSeverity,
};
use crate::network::event_bus::Event;
use crate::position::PositionManager;
use crate::state::SharedState;

/// Trading agent that coordinates all components with agentic capabilities
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
    
    // === Agentic Components ===
    /// Performance database for learning
    performance_db: Arc<PerformanceDb>,
    /// Adaptive position sizer
    adaptive_sizer: Arc<AdaptiveSizer>,
    /// Funding reversal detector
    reversal_detector: Arc<ReversalDetector>,
    /// Current trade context (for recording outcomes)
    current_trade_context: Arc<RwLock<Option<TradeContext>>>,
}

/// Context for current open trade (used to record outcome on close)
#[derive(Debug, Clone)]
pub struct TradeContext {
    pub id: String,
    pub open_time: i64,
    pub size: f64,
    pub entry_spot: f64,
    pub entry_perp: f64,
    pub entry_basis: f64,
    pub entry_funding_apr: f64,
    pub entry_confidence: f64,
    pub accumulated_funding: f64,
}

impl TradingAgent {
    /// Create a new trading agent with agentic capabilities
    pub async fn new(
        config: Arc<AppConfig>,
        state: Arc<SharedState>,
        position_manager: Arc<PositionManager>,
        event_tx: broadcast::Sender<Event>,
    ) -> Result<Self> {
        let state_machine = Arc::new(RwLock::new(AgentStateMachine::new()));
        let risk_manager = Arc::new(RiskManager::new(config.clone(), state.clone()));
        let rebalancer = Arc::new(Rebalancer::new(
            config.clone(),
            state.clone(),
            position_manager.clone(),
        ));
        
        // Initialize agentic components
        let db_path = "data/performance.json";
        
        // Ensure data directory exists
        tokio::fs::create_dir_all("data").await.ok();
        
        let performance_db = Arc::new(
            PerformanceDb::new(db_path).await?
        );
        
        let adaptive_sizer = Arc::new(AdaptiveSizer::new(
            config.clone(),
            performance_db.clone(),
        ));
        
        let reversal_detector = Arc::new(ReversalDetector::new(
            config.clone(),
            state.clone(),
            event_tx.clone(),
        ));
        
        info!("Trading agent initialized with agentic features");
        
        // Log current performance metrics
        let metrics = performance_db.get_metrics().await;
        if metrics.total_trades > 0 {
            info!(
                "Loaded performance history: {} trades, {:.1}% win rate, {:.2} profit factor",
                metrics.total_trades, metrics.win_rate * 100.0, metrics.profit_factor
            );
        }
        
        Ok(Self {
            config,
            state,
            state_machine,
            risk_manager,
            rebalancer,
            position_manager,
            event_tx,
            running: Arc::new(RwLock::new(false)),
            performance_db,
            adaptive_sizer,
            reversal_detector,
            current_trade_context: Arc::new(RwLock::new(None)),
        })
    }
    
    /// Start the trading agent
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Trading agent starting with agentic features");
        
        // Start reversal detector
        self.reversal_detector.start().await?;
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let state_machine = self.state_machine.clone();
        let risk_manager = self.risk_manager.clone();
        let rebalancer = self.rebalancer.clone();
        let position_manager = self.position_manager.clone();
        let event_tx = self.event_tx.clone();
        let performance_db = self.performance_db.clone();
        let adaptive_sizer = self.adaptive_sizer.clone();
        let reversal_detector = self.reversal_detector.clone();
        let current_trade_context = self.current_trade_context.clone();
        
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
                
                // Check for funding reversal (agentic feature)
                if let Some(severity) = reversal_detector.get_reversal_severity().await {
                    match severity {
                        ReversalSeverity::Critical => {
                            // Force close on critical reversal
                            let mut sm = state_machine.write().await;
                            if sm.current_state() == AgentState::Monitoring {
                                warn!("Critical funding reversal - forcing position close");
                                sm.transition_to(AgentState::Closing);
                                continue;
                            }
                        }
                        ReversalSeverity::High => {
                            // Emit warning but don't force close
                            debug!("High severity funding reversal detected");
                        }
                        _ => {}
                    }
                }
                
                // Get current state
                let current_state = state_machine.read().await.current_state();
                
                match current_state {
                    AgentState::Idle => {
                        // Check for trade opportunities
                        if let Some(signal) = Self::check_for_signals(&state, &config).await {
                            info!("Trade signal detected: {:?}", signal);
                            
                            // Get adaptive sizing recommendation
                            let basis = state.get_basis_spread();
                            let funding_apr = state.funding_apr.load();
                            let sizing = adaptive_sizer.get_recommended_size(
                                basis,
                                funding_apr,
                                0.8, // Signal confidence
                            ).await;
                            
                            info!(
                                "Adaptive sizing: {:.2} SOL ({:.1}% of max) | Kelly: {:.1}% | Adjustments: {:?}",
                                sizing.size_sol,
                                sizing.size_pct_of_max,
                                sizing.kelly_fraction * 100.0,
                                sizing.adjustments
                            );
                            
                            // Store trade context for later recording
                            let trade_id = uuid::Uuid::new_v4().to_string();
                            *current_trade_context.write().await = Some(TradeContext {
                                id: trade_id,
                                open_time: chrono::Utc::now().timestamp_millis(),
                                size: sizing.size_sol,
                                entry_spot: state.spot_price.load(),
                                entry_perp: state.perp_mark_price.load(),
                                entry_basis: basis,
                                entry_funding_apr: funding_apr,
                                entry_confidence: sizing.confidence,
                                accumulated_funding: 0.0,
                            });
                            
                            let mut sm = state_machine.write().await;
                            sm.transition_to(AgentState::Opening);
                        }
                    }
                    
                    AgentState::Opening => {
                        // Get the adaptive size from context
                        let size = current_trade_context.read().await
                            .as_ref()
                            .map(|c| c.size)
                            .unwrap_or(100.0);
                        
                        // Execute opening trade
                        if config.paper_trading {
                            debug!("Paper trading: simulating open with size {:.2} SOL", size);
                            position_manager.simulate_open(
                                state.spot_price.load(),
                                size,
                            ).await;
                        }
                        
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Monitoring);
                    }
                    
                    AgentState::Monitoring => {
                        // Update accumulated funding in context
                        if let Some(ctx) = current_trade_context.write().await.as_mut() {
                            let funding_rate = state.current_funding_rate.load();
                            // Estimate funding accrual (simplified)
                            ctx.accumulated_funding += funding_rate * ctx.size * state.spot_price.load();
                        }
                        
                        // Check for close condition
                        let basis = state.get_basis_spread();
                        
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
                        let exit_spot = state.spot_price.load();
                        let exit_perp = state.perp_mark_price.load();
                        let exit_basis = state.get_basis_spread();
                        
                        // Execute closing trade
                        let pnl = if config.paper_trading {
                            debug!("Paper trading: simulating close");
                            position_manager.simulate_close(exit_spot).await
                        } else {
                            0.0 // Would get from actual execution
                        };
                        
                        // Record trade outcome (agentic learning)
                        if let Some(ctx) = current_trade_context.write().await.take() {
                            let close_time = chrono::Utc::now().timestamp_millis();
                            let hold_hours = (close_time - ctx.open_time) as f64 / 3600000.0;
                            
                            // Calculate component P&Ls
                            let spot_pnl = (exit_spot - ctx.entry_spot) * ctx.size;
                            let perp_pnl = (ctx.entry_perp - exit_perp) * ctx.size; // Short position
                            let total_pnl = spot_pnl + perp_pnl + ctx.accumulated_funding;
                            let notional = ctx.entry_spot * ctx.size;
                            let roi_pct = if notional > 0.0 { total_pnl / notional * 100.0 } else { 0.0 };
                            
                            let outcome = TradeOutcome {
                                id: ctx.id,
                                open_time: ctx.open_time,
                                close_time,
                                size: ctx.size,
                                entry_spot: ctx.entry_spot,
                                entry_perp: ctx.entry_perp,
                                exit_spot,
                                exit_perp,
                                entry_basis: ctx.entry_basis,
                                exit_basis,
                                entry_funding_apr: ctx.entry_funding_apr,
                                funding_collected: ctx.accumulated_funding,
                                spot_pnl,
                                perp_pnl,
                                total_pnl,
                                roi_pct,
                                hold_hours,
                                is_winner: total_pnl > 0.0,
                                close_reason: "basis_converged".to_string(),
                                entry_confidence: ctx.entry_confidence,
                            };
                            
                            if let Err(e) = performance_db.record_trade(outcome).await {
                                error!("Failed to record trade outcome: {}", e);
                            }
                            
                            // Recalculate adaptive sizing
                            adaptive_sizer.recalculate().await;
                        }
                        
                        let mut sm = state_machine.write().await;
                        sm.transition_to(AgentState::Idle);
                        
                        // Log P&L
                        let realized_pnl = position_manager.get_realized_pnl().await;
                        info!("Position closed. Trade P&L: ${:.2} | Total realized: ${:.2}", pnl, realized_pnl);
                        
                        // Log updated performance metrics
                        let metrics = performance_db.get_metrics().await;
                        info!(
                            "Performance update: {} trades | {:.1}% win rate | ${:.2} net P&L | {:.2} profit factor",
                            metrics.total_trades,
                            metrics.win_rate * 100.0,
                            metrics.net_pnl,
                            metrics.profit_factor
                        );
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
                            // Also check reversal detector before resuming
                            let reversal_active = reversal_detector.is_reversal_active().await;
                            if !reversal_active {
                                info!("Risk conditions cleared, resuming");
                                let mut sm = state_machine.write().await;
                                sm.transition_to(AgentState::Idle);
                                let _ = event_tx.send(Event::SystemResume);
                            } else {
                                debug!("Waiting for funding reversal to clear before resuming");
                            }
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
        self.reversal_detector.stop().await;
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
    
    /// Get performance database
    pub fn performance_db(&self) -> &Arc<PerformanceDb> {
        &self.performance_db
    }
    
    /// Get adaptive sizer
    pub fn adaptive_sizer(&self) -> &Arc<AdaptiveSizer> {
        &self.adaptive_sizer
    }
    
    /// Get reversal detector
    pub fn reversal_detector(&self) -> &Arc<ReversalDetector> {
        &self.reversal_detector
    }
    
    /// Get performance metrics
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        self.performance_db.get_metrics().await
    }
    
    /// Get adaptive sizing recommendation
    pub async fn get_sizing_recommendation(&self, confidence: f64) -> SizingRecommendation {
        let basis = self.state.get_basis_spread();
        let funding_apr = self.state.funding_apr.load();
        self.adaptive_sizer.get_recommended_size(basis, funding_apr, confidence).await
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
    
    /// Export trade history to CSV
    pub async fn export_trades(&self, path: &str) -> Result<()> {
        self.performance_db.export_csv(path).await
    }
}
