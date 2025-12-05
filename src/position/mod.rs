//! Position Management Module - Phase 5
//!
//! Provides position tracking and P&L calculation:
//! - Spot and perp position tracking
//! - Realized and unrealized P&L
//! - Entry/exit price tracking
//! - Paper trading simulation

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};

use crate::state::SharedState;

/// Spot position
#[derive(Debug, Clone, Default)]
pub struct SpotPosition {
    /// Size in SOL
    pub size: f64,
    /// Average entry price
    pub entry_price: f64,
    /// Current value
    pub current_value: f64,
    /// Unrealized P&L
    pub unrealized_pnl: f64,
    /// Entry timestamp
    pub entry_time: i64,
}

/// Perp position
#[derive(Debug, Clone, Default)]
pub struct PerpPosition {
    /// Size in contracts (positive = long, negative = short)
    pub size: f64,
    /// Average entry price
    pub entry_price: f64,
    /// Current mark price
    pub mark_price: f64,
    /// Unrealized P&L
    pub unrealized_pnl: f64,
    /// Accumulated funding
    pub accumulated_funding: f64,
    /// Entry timestamp
    pub entry_time: i64,
}

/// Combined positions summary
#[derive(Debug, Clone, Default)]
pub struct PositionSummary {
    /// Spot size
    pub spot_size: f64,
    /// Perp size
    pub perp_size: f64,
    /// Spot entry price
    pub spot_entry: f64,
    /// Perp entry price
    pub perp_entry: f64,
    /// Total unrealized P&L
    pub unrealized_pnl: f64,
    /// Total realized P&L
    pub realized_pnl: f64,
    /// Hedge ratio
    pub hedge_ratio: f64,
    /// Position open time
    pub open_time: i64,
}

/// Position manager
pub struct PositionManager {
    /// Shared state
    state: Arc<SharedState>,
    /// Spot position
    spot: RwLock<Option<SpotPosition>>,
    /// Perp position
    perp: RwLock<Option<PerpPosition>>,
    /// Realized P&L
    realized_pnl: RwLock<f64>,
    /// Trade history
    trade_history: RwLock<Vec<TradeRecord>>,
}

/// Trade record
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub timestamp: i64,
    pub side: String,
    pub size: f64,
    pub price: f64,
    pub pnl: f64,
    pub trade_type: TradeType,
}

/// Trade type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeType {
    Open,
    Close,
    Rebalance,
}

impl PositionManager {
    /// Create a new position manager
    pub fn new(state: Arc<SharedState>) -> Self {
        Self {
            state,
            spot: RwLock::new(None),
            perp: RwLock::new(None),
            realized_pnl: RwLock::new(0.0),
            trade_history: RwLock::new(Vec::new()),
        }
    }
    
    /// Simulate opening a position (paper trading)
    pub async fn simulate_open(&self, spot_price: f64, size: f64) {
        let now = chrono::Utc::now().timestamp_millis();
        
        // Open spot long
        *self.spot.write().await = Some(SpotPosition {
            size,
            entry_price: spot_price,
            current_value: size * spot_price,
            unrealized_pnl: 0.0,
            entry_time: now,
        });
        
        // Open perp short (hedge)
        let perp_price = self.state.perp_mark_price.load();
        *self.perp.write().await = Some(PerpPosition {
            size: -size, // Negative for short
            entry_price: perp_price,
            mark_price: perp_price,
            unrealized_pnl: 0.0,
            accumulated_funding: 0.0,
            entry_time: now,
        });
        
        // Update shared state
        *self.state.spot_position.write() = Some(crate::utils::types::Position {
            size,
            entry_price: spot_price,
            side: crate::utils::types::PositionSide::Long,
            timestamp: now,
            unrealized_pnl: 0.0,
        });
        
        *self.state.perp_position.write() = Some(crate::utils::types::Position {
            size,
            entry_price: perp_price,
            side: crate::utils::types::PositionSide::Short,
            timestamp: now,
            unrealized_pnl: 0.0,
        });
        
        // Record trade
        self.record_trade(TradeRecord {
            timestamp: now,
            side: "OPEN".to_string(),
            size,
            price: spot_price,
            pnl: 0.0,
            trade_type: TradeType::Open,
        }).await;
        
        info!(
            "Opened position: {:.4} SOL @ ${:.2} spot, short perp @ ${:.2}",
            size, spot_price, perp_price
        );
    }
    
    /// Simulate closing a position (paper trading)
    pub async fn simulate_close(&self, current_price: f64) -> f64 {
        let now = chrono::Utc::now().timestamp_millis();
        let mut total_pnl = 0.0;
        
        // Close spot
        if let Some(spot) = self.spot.read().await.as_ref() {
            let spot_pnl = (current_price - spot.entry_price) * spot.size;
            total_pnl += spot_pnl;
            
            self.record_trade(TradeRecord {
                timestamp: now,
                side: "CLOSE_SPOT".to_string(),
                size: spot.size,
                price: current_price,
                pnl: spot_pnl,
                trade_type: TradeType::Close,
            }).await;
        }
        
        // Close perp
        if let Some(perp) = self.perp.read().await.as_ref() {
            let perp_price = self.state.perp_mark_price.load();
            // Short position: profit when price goes down
            let perp_pnl = (perp.entry_price - perp_price) * perp.size.abs();
            let funding_pnl = perp.accumulated_funding;
            total_pnl += perp_pnl + funding_pnl;
            
            self.record_trade(TradeRecord {
                timestamp: now,
                side: "CLOSE_PERP".to_string(),
                size: perp.size.abs(),
                price: perp_price,
                pnl: perp_pnl + funding_pnl,
                trade_type: TradeType::Close,
            }).await;
        }
        
        // Clear positions
        *self.spot.write().await = None;
        *self.perp.write().await = None;
        *self.state.spot_position.write() = None;
        *self.state.perp_position.write() = None;
        
        // Update realized P&L
        *self.realized_pnl.write().await += total_pnl;
        self.state.realized_pnl.store(self.state.realized_pnl.load() + total_pnl);
        
        info!("Closed position with P&L: ${:.2}", total_pnl);
        
        total_pnl
    }
    
    /// Adjust positions (for rebalancing)
    pub async fn adjust_positions(&self, spot_delta: f64, perp_delta: f64) {
        let now = chrono::Utc::now().timestamp_millis();
        
        // Adjust spot
        if let Some(spot) = self.spot.write().await.as_mut() {
            spot.size += spot_delta;
            debug!("Adjusted spot by {:.4}, new size: {:.4}", spot_delta, spot.size);
        }
        
        // Adjust perp
        if let Some(perp) = self.perp.write().await.as_mut() {
            perp.size += perp_delta;
            debug!("Adjusted perp by {:.4}, new size: {:.4}", perp_delta, perp.size);
        }
        
        // Record rebalance
        self.record_trade(TradeRecord {
            timestamp: now,
            side: "REBALANCE".to_string(),
            size: spot_delta.abs(),
            price: self.state.spot_price.load(),
            pnl: 0.0,
            trade_type: TradeType::Rebalance,
        }).await;
    }
    
    /// Update unrealized P&L based on current prices
    pub async fn update_pnl(&self) {
        let spot_price = self.state.spot_price.load();
        let perp_price = self.state.perp_mark_price.load();
        let mut total_unrealized = 0.0;
        
        // Update spot
        if let Some(spot) = self.spot.write().await.as_mut() {
            spot.current_value = spot.size * spot_price;
            spot.unrealized_pnl = (spot_price - spot.entry_price) * spot.size;
            total_unrealized += spot.unrealized_pnl;
        }
        
        // Update perp
        if let Some(perp) = self.perp.write().await.as_mut() {
            perp.mark_price = perp_price;
            // Short: profit when price goes down
            perp.unrealized_pnl = (perp.entry_price - perp_price) * perp.size.abs();
            total_unrealized += perp.unrealized_pnl + perp.accumulated_funding;
        }
        
        self.state.unrealized_pnl.store(total_unrealized);
    }
    
    /// Add funding payment
    pub async fn add_funding(&self, amount: f64) {
        if let Some(perp) = self.perp.write().await.as_mut() {
            perp.accumulated_funding += amount;
            debug!("Added funding: ${:.4}, total: ${:.4}", amount, perp.accumulated_funding);
        }
    }
    
    /// Get position summary
    pub async fn get_positions(&self) -> PositionSummary {
        let spot = self.spot.read().await;
        let perp = self.perp.read().await;
        
        let spot_size = spot.as_ref().map(|s| s.size).unwrap_or(0.0);
        let perp_size = perp.as_ref().map(|p| p.size.abs()).unwrap_or(0.0);
        
        let hedge_ratio = if spot_size > 0.0 {
            perp_size / spot_size
        } else {
            0.0
        };
        
        PositionSummary {
            spot_size,
            perp_size,
            spot_entry: spot.as_ref().map(|s| s.entry_price).unwrap_or(0.0),
            perp_entry: perp.as_ref().map(|p| p.entry_price).unwrap_or(0.0),
            unrealized_pnl: spot.as_ref().map(|s| s.unrealized_pnl).unwrap_or(0.0)
                + perp.as_ref().map(|p| p.unrealized_pnl + p.accumulated_funding).unwrap_or(0.0),
            realized_pnl: *self.realized_pnl.read().await,
            hedge_ratio,
            open_time: spot.as_ref().map(|s| s.entry_time).unwrap_or(0),
        }
    }
    
    /// Has open position
    pub async fn has_position(&self) -> bool {
        self.spot.read().await.is_some() || self.perp.read().await.is_some()
    }
    
    /// Get realized P&L
    pub async fn get_realized_pnl(&self) -> f64 {
        *self.realized_pnl.read().await
    }
    
    /// Record a trade
    async fn record_trade(&self, trade: TradeRecord) {
        let mut history = self.trade_history.write().await;
        history.push(trade);
        
        // Keep last 1000 trades
        if history.len() > 1000 {
            history.remove(0);
        }
    }
    
    /// Get trade history
    pub async fn get_trade_history(&self) -> Vec<TradeRecord> {
        self.trade_history.read().await.clone()
    }
    
    /// Get trade count
    pub async fn get_trade_count(&self) -> usize {
        self.trade_history.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_summary() {
        let summary = PositionSummary::default();
        assert_eq!(summary.spot_size, 0.0);
        assert_eq!(summary.hedge_ratio, 0.0);
    }
}
