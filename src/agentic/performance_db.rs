//! Performance Database
//!
//! SQLite-backed trade logging and metrics calculation:
//! - Stores all trade outcomes persistently
//! - Calculates win rate, Sharpe ratio, profit factor
//! - Tracks performance by market conditions
//! - Enables learning from historical performance

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Trade outcome record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOutcome {
    /// Unique trade ID
    pub id: String,
    /// Open timestamp (ms)
    pub open_time: i64,
    /// Close timestamp (ms)
    pub close_time: i64,
    /// Position size in SOL
    pub size: f64,
    /// Entry spot price
    pub entry_spot: f64,
    /// Entry perp price
    pub entry_perp: f64,
    /// Exit spot price
    pub exit_spot: f64,
    /// Exit perp price
    pub exit_perp: f64,
    /// Basis spread at entry (%)
    pub entry_basis: f64,
    /// Basis spread at exit (%)
    pub exit_basis: f64,
    /// Funding APR at entry
    pub entry_funding_apr: f64,
    /// Total funding collected
    pub funding_collected: f64,
    /// Spot P&L
    pub spot_pnl: f64,
    /// Perp P&L
    pub perp_pnl: f64,
    /// Total P&L (spot + perp + funding)
    pub total_pnl: f64,
    /// Return on capital (%)
    pub roi_pct: f64,
    /// Hold duration (hours)
    pub hold_hours: f64,
    /// Whether trade was profitable
    pub is_winner: bool,
    /// Close reason
    pub close_reason: String,
    /// Confidence score at entry
    pub entry_confidence: f64,
}

/// Performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total number of trades
    pub total_trades: u32,
    /// Number of winning trades
    pub winning_trades: u32,
    /// Number of losing trades
    pub losing_trades: u32,
    /// Win rate (0-1)
    pub win_rate: f64,
    /// Total profit from winners
    pub gross_profit: f64,
    /// Total loss from losers
    pub gross_loss: f64,
    /// Net P&L
    pub net_pnl: f64,
    /// Profit factor (gross_profit / gross_loss)
    pub profit_factor: f64,
    /// Average win
    pub avg_win: f64,
    /// Average loss
    pub avg_loss: f64,
    /// Expectancy per trade
    pub expectancy: f64,
    /// Average hold time (hours)
    pub avg_hold_hours: f64,
    /// Sharpe ratio (annualized)
    pub sharpe_ratio: f64,
    /// Maximum drawdown (%)
    pub max_drawdown_pct: f64,
    /// Average ROI per trade (%)
    pub avg_roi_pct: f64,
    /// Best trade P&L
    pub best_trade: f64,
    /// Worst trade P&L
    pub worst_trade: f64,
    /// Current streak (positive = wins, negative = losses)
    pub current_streak: i32,
    /// Longest win streak
    pub longest_win_streak: u32,
    /// Longest loss streak
    pub longest_loss_streak: u32,
}

/// Performance database using simple file storage
/// (SQLite would require additional dependency - using JSON for simplicity)
pub struct PerformanceDb {
    /// Database file path
    db_path: String,
    /// In-memory trades cache
    trades: Arc<RwLock<Vec<TradeOutcome>>>,
    /// Cached metrics
    metrics: Arc<RwLock<PerformanceMetrics>>,
}

impl PerformanceDb {
    /// Create or open a performance database
    pub async fn new(db_path: &str) -> Result<Self> {
        let trades = if Path::new(db_path).exists() {
            let content = tokio::fs::read_to_string(db_path).await
                .context("Failed to read performance database")?;
            serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };
        
        let db = Self {
            db_path: db_path.to_string(),
            trades: Arc::new(RwLock::new(trades)),
            metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
        };
        
        // Calculate initial metrics
        db.recalculate_metrics().await;
        
        info!("Performance database loaded: {} trades", db.trades.read().await.len());
        
        Ok(db)
    }
    
    /// Record a trade outcome
    pub async fn record_trade(&self, trade: TradeOutcome) -> Result<()> {
        {
            let mut trades = self.trades.write().await;
            trades.push(trade.clone());
        }
        
        // Persist to disk
        self.save().await?;
        
        // Recalculate metrics
        self.recalculate_metrics().await;
        
        info!(
            "Trade recorded: {} | P&L: ${:.2} | ROI: {:.2}% | Win: {}",
            trade.id, trade.total_pnl, trade.roi_pct, trade.is_winner
        );
        
        Ok(())
    }
    
    /// Save database to disk
    async fn save(&self) -> Result<()> {
        let trades = self.trades.read().await;
        let content = serde_json::to_string_pretty(&*trades)
            .context("Failed to serialize trades")?;
        
        tokio::fs::write(&self.db_path, content).await
            .context("Failed to write performance database")?;
        
        debug!("Performance database saved");
        Ok(())
    }
    
    /// Recalculate all metrics from trades
    async fn recalculate_metrics(&self) {
        let trades = self.trades.read().await;
        
        if trades.is_empty() {
            *self.metrics.write().await = PerformanceMetrics::default();
            return;
        }
        
        let total_trades = trades.len() as u32;
        let winning_trades = trades.iter().filter(|t| t.is_winner).count() as u32;
        let losing_trades = total_trades - winning_trades;
        
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };
        
        let gross_profit: f64 = trades.iter()
            .filter(|t| t.total_pnl > 0.0)
            .map(|t| t.total_pnl)
            .sum();
        
        let gross_loss: f64 = trades.iter()
            .filter(|t| t.total_pnl < 0.0)
            .map(|t| t.total_pnl.abs())
            .sum();
        
        let net_pnl: f64 = trades.iter().map(|t| t.total_pnl).sum();
        
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };
        
        let avg_win = if winning_trades > 0 {
            gross_profit / winning_trades as f64
        } else {
            0.0
        };
        
        let avg_loss = if losing_trades > 0 {
            gross_loss / losing_trades as f64
        } else {
            0.0
        };
        
        // Expectancy = (Win% × Avg Win) - (Loss% × Avg Loss)
        let expectancy = (win_rate * avg_win) - ((1.0 - win_rate) * avg_loss);
        
        let avg_hold_hours: f64 = trades.iter().map(|t| t.hold_hours).sum::<f64>() 
            / total_trades as f64;
        
        let avg_roi_pct: f64 = trades.iter().map(|t| t.roi_pct).sum::<f64>()
            / total_trades as f64;
        
        let best_trade = trades.iter().map(|t| t.total_pnl).fold(f64::NEG_INFINITY, f64::max);
        let worst_trade = trades.iter().map(|t| t.total_pnl).fold(f64::INFINITY, f64::min);
        
        // Calculate Sharpe ratio
        let returns: Vec<f64> = trades.iter().map(|t| t.roi_pct / 100.0).collect();
        let sharpe_ratio = Self::calculate_sharpe(&returns);
        
        // Calculate max drawdown
        let max_drawdown_pct = Self::calculate_max_drawdown(&trades);
        
        // Calculate streaks
        let (current_streak, longest_win, longest_loss) = Self::calculate_streaks(&trades);
        
        *self.metrics.write().await = PerformanceMetrics {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            gross_profit,
            gross_loss,
            net_pnl,
            profit_factor,
            avg_win,
            avg_loss,
            expectancy,
            avg_hold_hours,
            sharpe_ratio,
            max_drawdown_pct,
            avg_roi_pct,
            best_trade,
            worst_trade,
            current_streak,
            longest_win_streak: longest_win,
            longest_loss_streak: longest_loss,
        };
    }
    
    /// Calculate Sharpe ratio (annualized)
    fn calculate_sharpe(returns: &[f64]) -> f64 {
        if returns.len() < 2 {
            return 0.0;
        }
        
        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance: f64 = returns.iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();
        
        if std_dev == 0.0 {
            return 0.0;
        }
        
        // Annualize assuming ~100 trades per year
        let trades_per_year = 100.0;
        (mean / std_dev) * trades_per_year.sqrt()
    }
    
    /// Calculate maximum drawdown
    fn calculate_max_drawdown(trades: &[TradeOutcome]) -> f64 {
        if trades.is_empty() {
            return 0.0;
        }
        
        let mut peak = 0.0_f64;
        let mut max_dd = 0.0_f64;
        let mut cumulative = 0.0;
        
        for trade in trades {
            cumulative += trade.total_pnl;
            if cumulative > peak {
                peak = cumulative;
            }
            let drawdown = if peak > 0.0 {
                (peak - cumulative) / peak * 100.0
            } else {
                0.0
            };
            if drawdown > max_dd {
                max_dd = drawdown;
            }
        }
        
        max_dd
    }
    
    /// Calculate win/loss streaks
    fn calculate_streaks(trades: &[TradeOutcome]) -> (i32, u32, u32) {
        let mut current_streak: i32 = 0;
        let mut longest_win: u32 = 0;
        let mut longest_loss: u32 = 0;
        let mut current_win: u32 = 0;
        let mut current_loss: u32 = 0;
        
        for trade in trades {
            if trade.is_winner {
                current_win += 1;
                current_loss = 0;
                if current_win > longest_win {
                    longest_win = current_win;
                }
            } else {
                current_loss += 1;
                current_win = 0;
                if current_loss > longest_loss {
                    longest_loss = current_loss;
                }
            }
        }
        
        // Current streak
        if let Some(last) = trades.last() {
            if last.is_winner {
                current_streak = current_win as i32;
            } else {
                current_streak = -(current_loss as i32);
            }
        }
        
        (current_streak, longest_win, longest_loss)
    }
    
    /// Get current performance metrics
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        self.metrics.read().await.clone()
    }
    
    /// Get win rate
    pub async fn get_win_rate(&self) -> f64 {
        self.metrics.read().await.win_rate
    }
    
    /// Get recent win rate (last N trades)
    pub async fn get_recent_win_rate(&self, n: usize) -> f64 {
        let trades = self.trades.read().await;
        let recent: Vec<_> = trades.iter().rev().take(n).collect();
        
        if recent.is_empty() {
            return 0.5; // Default to 50% if no data
        }
        
        let wins = recent.iter().filter(|t| t.is_winner).count();
        wins as f64 / recent.len() as f64
    }
    
    /// Get average profit
    pub async fn get_avg_profit(&self) -> f64 {
        let metrics = self.metrics.read().await;
        if metrics.total_trades > 0 {
            metrics.net_pnl / metrics.total_trades as f64
        } else {
            0.0
        }
    }
    
    /// Get all trades
    pub async fn get_all_trades(&self) -> Vec<TradeOutcome> {
        self.trades.read().await.clone()
    }
    
    /// Get recent trades
    pub async fn get_recent_trades(&self, n: usize) -> Vec<TradeOutcome> {
        let trades = self.trades.read().await;
        trades.iter().rev().take(n).cloned().collect()
    }
    
    /// Get trades by time range
    pub async fn get_trades_in_range(&self, start: i64, end: i64) -> Vec<TradeOutcome> {
        let trades = self.trades.read().await;
        trades.iter()
            .filter(|t| t.open_time >= start && t.open_time <= end)
            .cloned()
            .collect()
    }
    
    /// Get performance by funding level
    pub async fn get_performance_by_funding(&self) -> FundingPerformance {
        let trades = self.trades.read().await;
        
        let high_funding: Vec<_> = trades.iter()
            .filter(|t| t.entry_funding_apr >= 25.0)
            .collect();
        let medium_funding: Vec<_> = trades.iter()
            .filter(|t| t.entry_funding_apr >= 15.0 && t.entry_funding_apr < 25.0)
            .collect();
        let low_funding: Vec<_> = trades.iter()
            .filter(|t| t.entry_funding_apr < 15.0)
            .collect();
        
        FundingPerformance {
            high_funding_win_rate: Self::win_rate_of(&high_funding),
            medium_funding_win_rate: Self::win_rate_of(&medium_funding),
            low_funding_win_rate: Self::win_rate_of(&low_funding),
            high_funding_avg_pnl: Self::avg_pnl_of(&high_funding),
            medium_funding_avg_pnl: Self::avg_pnl_of(&medium_funding),
            low_funding_avg_pnl: Self::avg_pnl_of(&low_funding),
        }
    }
    
    fn win_rate_of(trades: &[&TradeOutcome]) -> f64 {
        if trades.is_empty() {
            return 0.0;
        }
        let wins = trades.iter().filter(|t| t.is_winner).count();
        wins as f64 / trades.len() as f64
    }
    
    fn avg_pnl_of(trades: &[&TradeOutcome]) -> f64 {
        if trades.is_empty() {
            return 0.0;
        }
        trades.iter().map(|t| t.total_pnl).sum::<f64>() / trades.len() as f64
    }
    
    /// Export to CSV
    pub async fn export_csv(&self, path: &str) -> Result<()> {
        let trades = self.trades.read().await;
        let mut csv = String::from(
            "id,open_time,close_time,size,entry_spot,entry_perp,exit_spot,exit_perp,\
             entry_basis,exit_basis,entry_funding_apr,funding_collected,spot_pnl,perp_pnl,\
             total_pnl,roi_pct,hold_hours,is_winner,close_reason,entry_confidence\n"
        );
        
        for t in trades.iter() {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                t.id, t.open_time, t.close_time, t.size, t.entry_spot, t.entry_perp,
                t.exit_spot, t.exit_perp, t.entry_basis, t.exit_basis, t.entry_funding_apr,
                t.funding_collected, t.spot_pnl, t.perp_pnl, t.total_pnl, t.roi_pct,
                t.hold_hours, t.is_winner, t.close_reason, t.entry_confidence
            ));
        }
        
        tokio::fs::write(path, csv).await?;
        info!("Exported {} trades to {}", trades.len(), path);
        Ok(())
    }
}

/// Performance breakdown by funding level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingPerformance {
    pub high_funding_win_rate: f64,
    pub medium_funding_win_rate: f64,
    pub low_funding_win_rate: f64,
    pub high_funding_avg_pnl: f64,
    pub medium_funding_avg_pnl: f64,
    pub low_funding_avg_pnl: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpe_calculation() {
        let returns = vec![0.01, 0.02, -0.005, 0.015, 0.01];
        let sharpe = PerformanceDb::calculate_sharpe(&returns);
        assert!(sharpe > 0.0);
    }
    
    #[test]
    fn test_max_drawdown() {
        let trades = vec![
            TradeOutcome {
                id: "1".to_string(),
                total_pnl: 100.0,
                is_winner: true,
                ..Default::default()
            },
            TradeOutcome {
                id: "2".to_string(),
                total_pnl: -50.0,
                is_winner: false,
                ..Default::default()
            },
        ];
        let dd = PerformanceDb::calculate_max_drawdown(&trades);
        assert!(dd > 0.0);
    }
}

impl Default for TradeOutcome {
    fn default() -> Self {
        Self {
            id: String::new(),
            open_time: 0,
            close_time: 0,
            size: 0.0,
            entry_spot: 0.0,
            entry_perp: 0.0,
            exit_spot: 0.0,
            exit_perp: 0.0,
            entry_basis: 0.0,
            exit_basis: 0.0,
            entry_funding_apr: 0.0,
            funding_collected: 0.0,
            spot_pnl: 0.0,
            perp_pnl: 0.0,
            total_pnl: 0.0,
            roi_pct: 0.0,
            hold_hours: 0.0,
            is_winner: false,
            close_reason: String::new(),
            entry_confidence: 0.0,
        }
    }
}
