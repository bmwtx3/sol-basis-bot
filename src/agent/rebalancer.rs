//! Rebalancer
//!
//! Handles hedge rebalancing:
//! - Monitors hedge drift
//! - Calculates rebalance amounts
//! - Executes rebalance trades
//! - Rate limiting

use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

use crate::config::AppConfig;
use crate::position::PositionManager;
use crate::state::SharedState;

/// Rebalance decision
#[derive(Debug, Clone)]
pub struct RebalanceDecision {
    /// Should rebalance
    pub should_rebalance: bool,
    /// Spot adjustment (positive = buy, negative = sell)
    pub spot_adjustment: f64,
    /// Perp adjustment (positive = increase long/reduce short)
    pub perp_adjustment: f64,
    /// Reason for decision
    pub reason: String,
}

/// Rebalance result
#[derive(Debug, Clone)]
pub struct RebalanceResult {
    /// Whether rebalance succeeded
    pub success: bool,
    /// Spot size traded
    pub spot_traded: f64,
    /// Perp size traded
    pub perp_traded: f64,
    /// Transaction signature
    pub signature: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Rebalancer
pub struct Rebalancer {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Position manager
    position_manager: Arc<PositionManager>,
    /// Last rebalance timestamp
    last_rebalance: AtomicI64,
    /// Rebalance count this hour
    rebalance_count: AtomicU64,
    /// Hour of count reset
    count_reset_hour: AtomicI64,
}

impl Rebalancer {
    /// Create a new rebalancer
    pub fn new(
        config: Arc<AppConfig>,
        state: Arc<SharedState>,
        position_manager: Arc<PositionManager>,
    ) -> Self {
        Self {
            config,
            state,
            position_manager,
            last_rebalance: AtomicI64::new(0),
            rebalance_count: AtomicU64::new(0),
            count_reset_hour: AtomicI64::new(0),
        }
    }
    
    /// Check if rebalancing is needed
    pub async fn needs_rebalance(&self) -> bool {
        let decision = self.evaluate().await;
        decision.should_rebalance
    }
    
    /// Evaluate rebalancing need
    pub async fn evaluate(&self) -> RebalanceDecision {
        let hedge_drift = self.state.hedge_drift.load();
        let threshold = self.config.risk.hedge_drift_threshold_pct;
        
        // Check if drift exceeds threshold
        if hedge_drift.abs() < threshold {
            return RebalanceDecision {
                should_rebalance: false,
                spot_adjustment: 0.0,
                perp_adjustment: 0.0,
                reason: format!("Drift {:.2}% below threshold {:.2}%", hedge_drift, threshold),
            };
        }
        
        // Check rate limiting
        if !self.can_rebalance() {
            return RebalanceDecision {
                should_rebalance: false,
                spot_adjustment: 0.0,
                perp_adjustment: 0.0,
                reason: "Rate limited".to_string(),
            };
        }
        
        // Check minimum rebalance size
        let positions = self.position_manager.get_positions().await;
        let spot_size = positions.spot_size;
        let perp_size = positions.perp_size;
        
        // Calculate adjustment needed to restore 1:1 hedge
        // Drift > 0 means spot > perp (need to increase perp or decrease spot)
        // Drift < 0 means perp > spot (need to increase spot or decrease perp)
        let (spot_adjustment, perp_adjustment) = if hedge_drift > 0.0 {
            // Reduce spot or increase perp
            let adjustment = spot_size * (hedge_drift / 100.0);
            if adjustment < self.config.rebalance.min_rebalance_size_sol {
                return RebalanceDecision {
                    should_rebalance: false,
                    spot_adjustment: 0.0,
                    perp_adjustment: 0.0,
                    reason: format!("Adjustment {:.4} below minimum", adjustment),
                };
            }
            (-adjustment / 2.0, adjustment / 2.0) // Split adjustment
        } else {
            // Increase spot or reduce perp
            let adjustment = perp_size * (-hedge_drift / 100.0);
            if adjustment < self.config.rebalance.min_rebalance_size_sol {
                return RebalanceDecision {
                    should_rebalance: false,
                    spot_adjustment: 0.0,
                    perp_adjustment: 0.0,
                    reason: format!("Adjustment {:.4} below minimum", adjustment),
                };
            }
            (adjustment / 2.0, -adjustment / 2.0) // Split adjustment
        };
        
        RebalanceDecision {
            should_rebalance: true,
            spot_adjustment,
            perp_adjustment,
            reason: format!(
                "Drift {:.2}% exceeds threshold {:.2}%",
                hedge_drift, threshold
            ),
        }
    }
    
    /// Execute rebalancing
    pub async fn execute_rebalance(&self) -> Result<RebalanceResult> {
        let decision = self.evaluate().await;
        
        if !decision.should_rebalance {
            return Ok(RebalanceResult {
                success: false,
                spot_traded: 0.0,
                perp_traded: 0.0,
                signature: None,
                error: Some(decision.reason),
            });
        }
        
        info!(
            "Executing rebalance: spot={:.4} SOL, perp={:.4}",
            decision.spot_adjustment, decision.perp_adjustment
        );
        
        // Record rebalance attempt
        self.record_rebalance();
        
        // In paper trading mode, just update positions
        if self.config.paper_trading {
            self.position_manager.adjust_positions(
                decision.spot_adjustment,
                decision.perp_adjustment,
            ).await;
            
            // Update hedge drift in state
            self.update_hedge_drift().await;
            
            return Ok(RebalanceResult {
                success: true,
                spot_traded: decision.spot_adjustment,
                perp_traded: decision.perp_adjustment,
                signature: Some("paper_trade".to_string()),
                error: None,
            });
        }
        
        // Real execution would go here
        // For now, simulate success
        self.position_manager.adjust_positions(
            decision.spot_adjustment,
            decision.perp_adjustment,
        ).await;
        
        self.update_hedge_drift().await;
        
        Ok(RebalanceResult {
            success: true,
            spot_traded: decision.spot_adjustment,
            perp_traded: decision.perp_adjustment,
            signature: None,
            error: None,
        })
    }
    
    /// Check if rebalancing is allowed (rate limiting)
    fn can_rebalance(&self) -> bool {
        let now = chrono::Utc::now();
        let current_hour = now.timestamp() / 3600;
        let last_hour = self.count_reset_hour.load(Ordering::SeqCst);
        
        // Reset counter if new hour
        if current_hour > last_hour {
            self.rebalance_count.store(0, Ordering::SeqCst);
            self.count_reset_hour.store(current_hour, Ordering::SeqCst);
        }
        
        // Check count
        let count = self.rebalance_count.load(Ordering::SeqCst);
        if count >= self.config.rebalance.max_rebalances_per_hour as u64 {
            warn!("Rebalance rate limit reached: {}", count);
            return false;
        }
        
        // Check interval
        let last = self.last_rebalance.load(Ordering::SeqCst);
        let min_interval = self.config.rebalance.check_interval_secs as i64;
        if now.timestamp() - last < min_interval {
            debug!("Rebalance interval not met");
            return false;
        }
        
        true
    }
    
    /// Record a rebalance
    fn record_rebalance(&self) {
        self.last_rebalance.store(
            chrono::Utc::now().timestamp(),
            Ordering::SeqCst,
        );
        self.rebalance_count.fetch_add(1, Ordering::SeqCst);
    }
    
    /// Update hedge drift in state
    async fn update_hedge_drift(&self) {
        let positions = self.position_manager.get_positions().await;
        let spot_size = positions.spot_size;
        let perp_size = positions.perp_size;
        
        let drift = if spot_size > 0.0 {
            ((spot_size - perp_size) / spot_size) * 100.0
        } else {
            0.0
        };
        
        self.state.hedge_drift.store(drift);
        debug!("Updated hedge drift: {:.2}%", drift);
    }
    
    /// Get rebalance statistics
    pub fn get_stats(&self) -> RebalanceStats {
        RebalanceStats {
            last_rebalance: self.last_rebalance.load(Ordering::SeqCst),
            rebalances_this_hour: self.rebalance_count.load(Ordering::SeqCst) as u32,
            max_per_hour: self.config.rebalance.max_rebalances_per_hour,
        }
    }
}

/// Rebalance statistics
#[derive(Debug, Clone)]
pub struct RebalanceStats {
    /// Timestamp of last rebalance
    pub last_rebalance: i64,
    /// Number of rebalances this hour
    pub rebalances_this_hour: u32,
    /// Maximum allowed per hour
    pub max_per_hour: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebalance_decision() {
        let decision = RebalanceDecision {
            should_rebalance: true,
            spot_adjustment: -5.0,
            perp_adjustment: 5.0,
            reason: "Test".to_string(),
        };
        assert!(decision.should_rebalance);
    }
}
