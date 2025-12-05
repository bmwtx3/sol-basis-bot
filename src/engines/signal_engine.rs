//! Signal Generation Engine
//!
//! Combines funding and basis analysis to generate trade signals:
//! - Open basis trade signals
//! - Close basis trade signals
//! - Rebalance signals
//! - Risk-adjusted position sizing

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;
use crate::utils::types::{SignalType, TradeSignal};

use super::funding_engine::FundingAnalysis;
use super::basis_engine::BasisAnalysis;

/// Signal evaluation result
#[derive(Debug, Clone)]
pub struct SignalEvaluation {
    /// Should open a new basis trade
    pub should_open: bool,
    /// Should close existing basis trade
    pub should_close: bool,
    /// Should rebalance hedge
    pub should_rebalance: bool,
    /// Recommended position size in SOL
    pub recommended_size: f64,
    /// Confidence score (0-1)
    pub confidence: f64,
    /// Expected profit in USD
    pub expected_profit: f64,
    /// Reasons for the signal
    pub reasons: Vec<String>,
    /// Timestamp
    pub timestamp: i64,
}

/// Trade signal with full context
#[derive(Debug, Clone)]
pub struct FullTradeSignal {
    /// Signal type
    pub signal: TradeSignal,
    /// Funding analysis at time of signal
    pub funding: Option<FundingAnalysis>,
    /// Basis analysis at time of signal
    pub basis: Option<BasisAnalysis>,
    /// Evaluation result
    pub evaluation: SignalEvaluation,
}

/// Signal generation engine
pub struct SignalEngine {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Last signal
    last_signal: Arc<RwLock<Option<FullTradeSignal>>>,
    /// Signal history
    signal_history: Arc<RwLock<Vec<FullTradeSignal>>>,
}

impl SignalEngine {
    /// Create a new signal engine
    pub fn new(
        config: Arc<AppConfig>,
        state: Arc<SharedState>,
        event_tx: broadcast::Sender<Event>,
    ) -> Self {
        Self {
            config,
            state,
            event_tx,
            running: Arc::new(RwLock::new(false)),
            last_signal: Arc::new(RwLock::new(None)),
            signal_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Start the signal engine
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Signal engine starting");
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let event_tx = self.event_tx.clone();
        let last_signal = self.last_signal.clone();
        let signal_history = self.signal_history.clone();
        
        tokio::spawn(async move {
            // Evaluate signals every 5 seconds
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            
            while *running.read().await {
                interval.tick().await;
                
                // Get current market state
                let spot_price = state.spot_price.load();
                let perp_price = state.perp_mark_price.load();
                let basis_spread = state.get_basis_spread();
                let funding_apr = state.funding_apr.load();
                let timestamp = chrono::Utc::now().timestamp_millis();
                
                if spot_price <= 0.0 || perp_price <= 0.0 {
                    continue;
                }
                
                // Check if we have open positions
                let has_positions = state.spot_position.read().is_some() 
                    || state.perp_position.read().is_some();
                
                // Evaluate trading conditions
                let evaluation = Self::evaluate_conditions(
                    &config,
                    &state,
                    basis_spread,
                    funding_apr,
                    has_positions,
                    timestamp,
                ).await;
                
                // Generate signal if conditions met
                if evaluation.should_open || evaluation.should_close || evaluation.should_rebalance {
                    let signal_type = if evaluation.should_open {
                        SignalType::OpenBasis
                    } else if evaluation.should_close {
                        SignalType::CloseBasis
                    } else {
                        SignalType::Rebalance
                    };
                    
                    let trade_signal = TradeSignal {
                        signal_type,
                        size: evaluation.recommended_size,
                        basis_spread,
                        funding_apr,
                        expected_profit: evaluation.expected_profit,
                        confidence: evaluation.confidence,
                        timestamp,
                        reason: evaluation.reasons.join("; "),
                    };
                    
                    let full_signal = FullTradeSignal {
                        signal: trade_signal.clone(),
                        funding: None, // Would be populated from funding engine
                        basis: None,   // Would be populated from basis engine
                        evaluation: evaluation.clone(),
                    };
                    
                    // Store signal
                    *last_signal.write().await = Some(full_signal.clone());
                    
                    // Add to history (keep last 100)
                    {
                        let mut history = signal_history.write().await;
                        history.push(full_signal);
                        if history.len() > 100 {
                            history.remove(0);
                        }
                    }
                    
                    info!(
                        "Signal generated: {:?} | Size: {:.2} SOL | Confidence: {:.1}% | Reason: {}",
                        signal_type,
                        evaluation.recommended_size,
                        evaluation.confidence * 100.0,
                        evaluation.reasons.join("; ")
                    );
                    
                    // Emit event
                    let _ = event_tx.send(Event::TradeSignal {
                        signal_type: format!("{:?}", signal_type),
                        size: evaluation.recommended_size,
                        reason: evaluation.reasons.join("; "),
                    });
                }
            }
            
            info!("Signal engine stopped");
        });
        
        Ok(())
    }
    
    /// Evaluate trading conditions
    async fn evaluate_conditions(
        config: &Arc<AppConfig>,
        state: &Arc<SharedState>,
        basis_spread: f64,
        funding_apr: f64,
        has_positions: bool,
        timestamp: i64,
    ) -> SignalEvaluation {
        let mut reasons = Vec::new();
        let mut confidence = 0.0;
        let mut should_open = false;
        let mut should_close = false;
        let mut should_rebalance = false;
        
        let min_basis = config.trading.min_basis_spread_pct;
        let min_funding = config.trading.min_funding_apr_pct;
        let close_threshold = config.trading.basis_close_threshold_pct;
        let hedge_drift_threshold = config.risk.hedge_drift_threshold_pct;
        
        // Check open conditions (no existing position)
        if !has_positions {
            // Check basis spread
            if basis_spread.abs() >= min_basis {
                confidence += 0.3;
                reasons.push(format!("Basis {:.3}% >= {:.3}%", basis_spread, min_basis));
                
                // Check funding APR
                if funding_apr.abs() >= min_funding {
                    confidence += 0.3;
                    reasons.push(format!("Funding APR {:.1}% >= {:.1}%", funding_apr, min_funding));
                    
                    // Check alignment (basis and funding same direction)
                    let aligned = (basis_spread > 0.0 && funding_apr > 0.0) ||
                                 (basis_spread < 0.0 && funding_apr < 0.0);
                    if aligned {
                        confidence += 0.2;
                        reasons.push("Basis and funding aligned".to_string());
                    }
                    
                    // Check time since last trade
                    let last_trade = state.last_trade.load(std::sync::atomic::Ordering::SeqCst);
                    let time_since_trade = timestamp - last_trade;
                    if time_since_trade > (config.risk.min_trade_interval_secs as i64 * 1000) {
                        confidence += 0.2;
                        should_open = true;
                    } else {
                        reasons.push("Too soon since last trade".to_string());
                    }
                }
            }
        } else {
            // Check close conditions (has existing position)
            
            // Basis convergence
            if basis_spread.abs() <= close_threshold {
                confidence += 0.5;
                reasons.push(format!("Basis converged to {:.4}%", basis_spread));
                should_close = true;
            }
            
            // Funding reversal
            // (Would need to track funding direction change)
            
            // Hedge drift
            let hedge_drift = state.hedge_drift.load();
            if hedge_drift.abs() > hedge_drift_threshold {
                confidence += 0.3;
                reasons.push(format!("Hedge drift {:.2}%", hedge_drift));
                should_rebalance = true;
            }
        }
        
        // Calculate position size
        let recommended_size = if should_open {
            Self::calculate_recommended_size(
                config,
                basis_spread,
                funding_apr,
                confidence,
            )
        } else {
            0.0
        };
        
        // Calculate expected profit (simplified)
        let expected_profit = if should_open {
            // Assume we capture half the basis over a week
            let notional = recommended_size * state.spot_price.load();
            notional * (basis_spread.abs() / 100.0) * 0.5
        } else {
            0.0
        };
        
        SignalEvaluation {
            should_open,
            should_close,
            should_rebalance,
            recommended_size,
            confidence: confidence.min(1.0),
            expected_profit,
            reasons,
            timestamp,
        }
    }
    
    /// Calculate recommended position size
    fn calculate_recommended_size(
        config: &Arc<AppConfig>,
        basis_spread: f64,
        funding_apr: f64,
        confidence: f64,
    ) -> f64 {
        let max_size = config.trading.max_position_size_sol;
        let min_basis = config.trading.min_basis_spread_pct;
        
        // Base size is 20% of max
        let base_size = max_size * 0.2;
        
        // Scale up based on spread strength
        let spread_multiple = (basis_spread.abs() / min_basis).min(3.0);
        
        // Scale up based on funding strength
        let funding_multiple = (funding_apr.abs() / config.trading.min_funding_apr_pct).min(2.0);
        
        // Apply confidence factor
        let size = base_size * spread_multiple * funding_multiple.sqrt() * confidence;
        
        // Clamp to max
        size.min(max_size)
    }
    
    /// Stop the signal engine
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Signal engine stopping");
    }
    
    /// Get last signal
    pub async fn get_last_signal(&self) -> Option<FullTradeSignal> {
        self.last_signal.read().await.clone()
    }
    
    /// Get signal history
    pub async fn get_signal_history(&self) -> Vec<FullTradeSignal> {
        self.signal_history.read().await.clone()
    }
    
    /// Get number of signals generated
    pub async fn get_signal_count(&self) -> usize {
        self.signal_history.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_evaluation() {
        let eval = SignalEvaluation {
            should_open: true,
            should_close: false,
            should_rebalance: false,
            recommended_size: 10.0,
            confidence: 0.8,
            expected_profit: 50.0,
            reasons: vec!["Test".to_string()],
            timestamp: 0,
        };
        assert!(eval.should_open);
        assert!(!eval.should_close);
    }
}
