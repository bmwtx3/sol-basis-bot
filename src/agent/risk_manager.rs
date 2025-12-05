//! Risk Manager
//!
//! Monitors and enforces risk limits:
//! - Maximum drawdown
//! - Stop loss per position
//! - Position size limits
//! - Daily loss limits
//! - Circuit breakers

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

use crate::config::AppConfig;
use crate::state::SharedState;

/// Risk check result
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    /// Should pause trading
    pub should_pause: bool,
    /// Should close positions
    pub should_close: bool,
    /// Reasons for the decision
    pub reasons: Vec<String>,
    /// Risk score (0-100, higher = more risky)
    pub risk_score: f64,
}

/// Risk metrics
#[derive(Debug, Clone)]
pub struct RiskMetrics {
    /// Current drawdown percentage
    pub drawdown_pct: f64,
    /// Peak equity
    pub peak_equity: f64,
    /// Current equity
    pub current_equity: f64,
    /// Unrealized P&L
    pub unrealized_pnl: f64,
    /// Realized P&L today
    pub realized_pnl_today: f64,
    /// Number of trades today
    pub trades_today: u32,
    /// Error count (last hour)
    pub error_count: u32,
}

/// Risk manager
pub struct RiskManager {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Peak equity (high water mark)
    peak_equity: AtomicU64,
    /// Daily P&L tracking
    daily_pnl: AtomicI64,
    /// Trade count today
    trades_today: AtomicU64,
    /// Last reset timestamp
    last_reset: AtomicI64,
    /// Is paused
    paused: RwLock<bool>,
    /// Pause reason
    pause_reason: RwLock<Option<String>>,
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new(config: Arc<AppConfig>, state: Arc<SharedState>) -> Self {
        Self {
            config,
            state,
            peak_equity: AtomicU64::new(0),
            daily_pnl: AtomicI64::new(0),
            trades_today: AtomicU64::new(0),
            last_reset: AtomicI64::new(chrono::Utc::now().timestamp()),
            paused: RwLock::new(false),
            pause_reason: RwLock::new(None),
        }
    }
    
    /// Perform all risk checks
    pub async fn check_all(&self) -> RiskCheckResult {
        let mut reasons = Vec::new();
        let mut should_pause = false;
        let mut should_close = false;
        let mut risk_score = 0.0;
        
        // Check daily reset
        self.check_daily_reset().await;
        
        // 1. Check drawdown
        let drawdown = self.calculate_drawdown().await;
        if drawdown >= self.config.risk.max_drawdown_pct {
            should_pause = true;
            should_close = true;
            reasons.push(format!("Max drawdown exceeded: {:.2}%", drawdown));
            risk_score += 50.0;
        } else if drawdown >= self.config.risk.max_drawdown_pct * 0.8 {
            reasons.push(format!("Drawdown warning: {:.2}%", drawdown));
            risk_score += 25.0;
        }
        
        // 2. Check position stop loss
        let unrealized_pnl = self.state.unrealized_pnl.load();
        let position_value = self.get_position_value().await;
        if position_value > 0.0 {
            let loss_pct = (-unrealized_pnl / position_value) * 100.0;
            if loss_pct >= self.config.risk.stop_loss_pct {
                should_close = true;
                reasons.push(format!("Stop loss triggered: {:.2}%", loss_pct));
                risk_score += 30.0;
            }
        }
        
        // 3. Check hedge drift
        let hedge_drift = self.state.hedge_drift.load().abs();
        if hedge_drift >= self.config.risk.hedge_drift_threshold_pct * 2.0 {
            should_pause = true;
            reasons.push(format!("Excessive hedge drift: {:.2}%", hedge_drift));
            risk_score += 20.0;
        }
        
        // 4. Check error rate
        let error_count = self.state.error_count.load(Ordering::SeqCst);
        if error_count > 10 {
            should_pause = true;
            reasons.push(format!("High error count: {}", error_count));
            risk_score += 15.0;
        }
        
        // 5. Check connection status
        if !*self.state.rpc_connected.read() {
            should_pause = true;
            reasons.push("RPC disconnected".to_string());
            risk_score += 25.0;
        }
        
        // 6. Check daily loss limit (implied from max_funding_reversal_loss)
        let daily_pnl = self.daily_pnl.load(Ordering::SeqCst) as f64 / 1_000_000.0;
        if daily_pnl < -self.config.risk.max_funding_reversal_loss {
            should_pause = true;
            reasons.push(format!("Daily loss limit: ${:.2}", daily_pnl));
            risk_score += 40.0;
        }
        
        // Update pause state
        if should_pause {
            *self.paused.write().await = true;
            *self.pause_reason.write().await = Some(reasons.join("; "));
        }
        
        RiskCheckResult {
            should_pause,
            should_close,
            reasons,
            risk_score: risk_score.min(100.0),
        }
    }
    
    /// Calculate current drawdown
    async fn calculate_drawdown(&self) -> f64 {
        let current_equity = self.get_current_equity().await;
        let peak = self.peak_equity.load(Ordering::SeqCst) as f64 / 1_000_000.0;
        
        if current_equity > peak {
            // New high water mark
            self.peak_equity.store(
                (current_equity * 1_000_000.0) as u64,
                Ordering::SeqCst,
            );
            0.0
        } else if peak > 0.0 {
            ((peak - current_equity) / peak) * 100.0
        } else {
            0.0
        }
    }
    
    /// Get current equity
    async fn get_current_equity(&self) -> f64 {
        let unrealized = self.state.unrealized_pnl.load();
        let realized = self.state.realized_pnl.load();
        
        // Assume starting capital of 10000 for now
        // In production, would track actual balance
        10000.0 + realized + unrealized
    }
    
    /// Get position value
    async fn get_position_value(&self) -> f64 {
        let spot_price = self.state.spot_price.load();
        // Simplified - would get actual position size
        spot_price * 100.0 // Assume 100 SOL position
    }
    
    /// Check and perform daily reset
    async fn check_daily_reset(&self) {
        let now = chrono::Utc::now();
        let last_reset = self.last_reset.load(Ordering::SeqCst);
        let last_date = chrono::DateTime::from_timestamp(last_reset, 0)
            .map(|dt| dt.date_naive())
            .unwrap_or_default();
        
        if now.date_naive() > last_date {
            info!("Daily reset triggered");
            self.daily_pnl.store(0, Ordering::SeqCst);
            self.trades_today.store(0, Ordering::SeqCst);
            self.last_reset.store(now.timestamp(), Ordering::SeqCst);
            self.state.error_count.store(0, Ordering::SeqCst);
        }
    }
    
    /// Record a trade
    pub fn record_trade(&self, pnl: f64) {
        self.trades_today.fetch_add(1, Ordering::SeqCst);
        let pnl_micro = (pnl * 1_000_000.0) as i64;
        self.daily_pnl.fetch_add(pnl_micro, Ordering::SeqCst);
    }
    
    /// Check if can resume trading
    pub async fn can_resume(&self) -> bool {
        // Check if conditions have improved
        let check = self.check_all().await;
        
        if !check.should_pause && check.risk_score < 30.0 {
            *self.paused.write().await = false;
            *self.pause_reason.write().await = None;
            true
        } else {
            false
        }
    }
    
    /// Is currently paused
    pub async fn is_paused(&self) -> bool {
        *self.paused.read().await
    }
    
    /// Get pause reason
    pub async fn pause_reason(&self) -> Option<String> {
        self.pause_reason.read().await.clone()
    }
    
    /// Get risk metrics
    pub async fn get_metrics(&self) -> RiskMetrics {
        let current_equity = self.get_current_equity().await;
        let peak = self.peak_equity.load(Ordering::SeqCst) as f64 / 1_000_000.0;
        let drawdown = if peak > 0.0 {
            ((peak - current_equity) / peak) * 100.0
        } else {
            0.0
        };
        
        RiskMetrics {
            drawdown_pct: drawdown,
            peak_equity: peak,
            current_equity,
            unrealized_pnl: self.state.unrealized_pnl.load(),
            realized_pnl_today: self.daily_pnl.load(Ordering::SeqCst) as f64 / 1_000_000.0,
            trades_today: self.trades_today.load(Ordering::SeqCst) as u32,
            error_count: self.state.error_count.load(Ordering::SeqCst),
        }
    }
    
    /// Force pause
    pub async fn force_pause(&self, reason: &str) {
        warn!("Force pause: {}", reason);
        *self.paused.write().await = true;
        *self.pause_reason.write().await = Some(reason.to_string());
    }
    
    /// Force resume (use with caution)
    pub async fn force_resume(&self) {
        warn!("Force resume");
        *self.paused.write().await = false;
        *self.pause_reason.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_check_result() {
        let result = RiskCheckResult {
            should_pause: false,
            should_close: false,
            reasons: vec![],
            risk_score: 0.0,
        };
        assert!(!result.should_pause);
    }
}
