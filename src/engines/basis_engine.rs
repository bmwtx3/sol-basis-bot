//! Basis Spread Engine
//!
//! Calculates and monitors:
//! - Real-time basis spread (perp vs spot)
//! - Optimal hedge ratio for delta-neutral
//! - Hedge drift detection
//! - Historical basis percentiles

use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;

/// Basis spread snapshot
#[derive(Debug, Clone)]
pub struct BasisSnapshot {
    pub timestamp: i64,
    pub spot_price: f64,
    pub perp_price: f64,
    pub spread_pct: f64,
}

/// Basis analysis result
#[derive(Debug, Clone)]
pub struct BasisAnalysis {
    /// Current spot price
    pub spot_price: f64,
    /// Current perp mark price
    pub perp_price: f64,
    /// Current basis spread percentage: (perp - spot) / spot * 100
    pub spread_pct: f64,
    /// Annualized basis yield
    pub annualized_yield: f64,
    /// 1-hour average spread
    pub avg_1h_spread: f64,
    /// 8-hour average spread
    pub avg_8h_spread: f64,
    /// Current percentile vs historical (0-100)
    pub percentile: f64,
    /// Standard deviation of spread
    pub std_dev: f64,
    /// Z-score (how many std devs from mean)
    pub z_score: f64,
    /// Optimal hedge ratio for delta-neutral
    pub hedge_ratio: f64,
    /// Current hedge drift (if positions exist)
    pub hedge_drift: f64,
    /// Is basis spread above minimum threshold
    pub is_tradeable: bool,
    /// Timestamp
    pub timestamp: i64,
}

/// Basis spread engine
pub struct BasisEngine {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Basis history (8-hour rolling window)
    history: Arc<RwLock<VecDeque<BasisSnapshot>>>,
    /// Last analysis result
    last_analysis: Arc<RwLock<Option<BasisAnalysis>>>,
}

impl BasisEngine {
    /// Create a new basis engine
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
            history: Arc::new(RwLock::new(VecDeque::with_capacity(2880))), // 8 hours at 10s intervals
            last_analysis: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the basis engine
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Basis engine starting");
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let event_tx = self.event_tx.clone();
        let history = self.history.clone();
        let last_analysis = self.last_analysis.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            while *running.read().await {
                interval.tick().await;
                
                let spot_price = state.spot_price.load();
                let perp_price = state.perp_mark_price.load();
                let timestamp = chrono::Utc::now().timestamp_millis();
                
                if spot_price > 0.0 && perp_price > 0.0 {
                    let spread_pct = ((perp_price - spot_price) / spot_price) * 100.0;
                    
                    // Add to history
                    {
                        let mut hist = history.write().await;
                        hist.push_back(BasisSnapshot {
                            timestamp,
                            spot_price,
                            perp_price,
                            spread_pct,
                        });
                        
                        // Keep only last 8 hours
                        let cutoff = timestamp - (8 * 60 * 60 * 1000);
                        while hist.front().map(|s| s.timestamp < cutoff).unwrap_or(false) {
                            hist.pop_front();
                        }
                    }
                    
                    // Perform analysis
                    let analysis = Self::analyze(
                        &history,
                        &state,
                        spot_price,
                        perp_price,
                        spread_pct,
                        config.trading.min_basis_spread_pct,
                        timestamp,
                    ).await;
                    
                    debug!(
                        "Basis analysis: spread={:.4}%, 1h_avg={:.4}%, percentile={:.1}, z={:.2}",
                        analysis.spread_pct,
                        analysis.avg_1h_spread,
                        analysis.percentile,
                        analysis.z_score
                    );
                    
                    // Store analysis
                    *last_analysis.write().await = Some(analysis.clone());
                    
                    // Emit basis update event
                    let _ = event_tx.send(Event::BasisSpreadUpdate {
                        spread: analysis.spread_pct,
                        spot_price,
                        perp_price,
                        timestamp,
                    });
                    
                    // Check for hedge drift alert
                    if analysis.hedge_drift.abs() > config.risk.hedge_drift_threshold_pct {
                        let _ = event_tx.send(Event::TradeSignal {
                            signal_type: "hedge_drift".to_string(),
                            size: 0.0,
                            reason: format!(
                                "Hedge drift {:.2}% exceeds threshold {:.2}%",
                                analysis.hedge_drift,
                                config.risk.hedge_drift_threshold_pct
                            ),
                        });
                    }
                }
            }
            
            info!("Basis engine stopped");
        });
        
        Ok(())
    }
    
    /// Analyze basis spread
    async fn analyze(
        history: &Arc<RwLock<VecDeque<BasisSnapshot>>>,
        state: &Arc<SharedState>,
        spot_price: f64,
        perp_price: f64,
        spread_pct: f64,
        min_spread: f64,
        timestamp: i64,
    ) -> BasisAnalysis {
        let hist = history.read().await;
        
        // Calculate averages
        let avg_1h = Self::calculate_avg(&hist, timestamp, 1);
        let avg_8h = Self::calculate_avg(&hist, timestamp, 8);
        
        // Calculate standard deviation and z-score
        let (std_dev, z_score) = Self::calculate_stats(&hist, spread_pct);
        
        // Calculate percentile
        let percentile = Self::calculate_percentile(&hist, spread_pct);
        
        // Calculate annualized yield (assuming 1-hour funding)
        // Basis yield = spread * 24 * 365 (simplified)
        let annualized_yield = spread_pct * 365.0;
        
        // Calculate optimal hedge ratio
        // For delta-neutral: hedge_ratio = 1.0 (equal and opposite positions)
        // Adjust based on funding direction for optimal carry
        let hedge_ratio = 1.0;
        
        // Calculate hedge drift from positions
        let hedge_drift = state.hedge_drift.load();
        
        // Check if tradeable
        let is_tradeable = spread_pct.abs() >= min_spread;
        
        BasisAnalysis {
            spot_price,
            perp_price,
            spread_pct,
            annualized_yield,
            avg_1h_spread: avg_1h,
            avg_8h_spread: avg_8h,
            percentile,
            std_dev,
            z_score,
            hedge_ratio,
            hedge_drift,
            is_tradeable,
            timestamp,
        }
    }
    
    /// Calculate average spread over N hours
    fn calculate_avg(history: &VecDeque<BasisSnapshot>, now: i64, hours: i64) -> f64 {
        let cutoff = now - (hours * 60 * 60 * 1000);
        let relevant: Vec<_> = history.iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();
        
        if relevant.is_empty() {
            return 0.0;
        }
        
        let sum: f64 = relevant.iter().map(|s| s.spread_pct).sum();
        sum / relevant.len() as f64
    }
    
    /// Calculate standard deviation and z-score
    fn calculate_stats(history: &VecDeque<BasisSnapshot>, current: f64) -> (f64, f64) {
        if history.len() < 2 {
            return (0.0, 0.0);
        }
        
        let mean: f64 = history.iter().map(|s| s.spread_pct).sum::<f64>() / history.len() as f64;
        let variance: f64 = history.iter()
            .map(|s| (s.spread_pct - mean).powi(2))
            .sum::<f64>() / history.len() as f64;
        let std_dev = variance.sqrt();
        
        let z_score = if std_dev > 0.0 {
            (current - mean) / std_dev
        } else {
            0.0
        };
        
        (std_dev, z_score)
    }
    
    /// Calculate percentile rank
    fn calculate_percentile(history: &VecDeque<BasisSnapshot>, current: f64) -> f64 {
        if history.is_empty() {
            return 50.0;
        }
        
        let count_below = history.iter()
            .filter(|s| s.spread_pct < current)
            .count();
        
        (count_below as f64 / history.len() as f64) * 100.0
    }
    
    /// Stop the basis engine
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Basis engine stopping");
    }
    
    /// Get last analysis
    pub async fn get_last_analysis(&self) -> Option<BasisAnalysis> {
        self.last_analysis.read().await.clone()
    }
    
    /// Get current spread
    pub async fn get_current_spread(&self) -> f64 {
        self.last_analysis.read().await
            .as_ref()
            .map(|a| a.spread_pct)
            .unwrap_or(0.0)
    }
    
    /// Check if basis is tradeable
    pub async fn is_tradeable(&self) -> bool {
        self.last_analysis.read().await
            .as_ref()
            .map(|a| a.is_tradeable)
            .unwrap_or(false)
    }
    
    /// Get optimal position size based on spread and risk
    pub fn calculate_position_size(
        &self,
        available_capital: f64,
        max_position: f64,
        current_spread: f64,
        min_spread: f64,
    ) -> f64 {
        if current_spread.abs() < min_spread {
            return 0.0;
        }
        
        // Scale position with spread (higher spread = larger position)
        let spread_multiple = (current_spread.abs() / min_spread).min(3.0);
        let base_size = available_capital * 0.1; // 10% of capital as base
        let scaled_size = base_size * spread_multiple;
        
        scaled_size.min(max_position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basis_calculation() {
        let spot = 100.0;
        let perp = 101.0;
        let spread = ((perp - spot) / spot) * 100.0;
        assert!((spread - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_position_sizing() {
        let engine = BasisEngine::new(
            Arc::new(AppConfig::default_for_test()),
            Arc::new(SharedState::new()),
            broadcast::channel(10).0,
        );
        
        // Below minimum spread
        let size = engine.calculate_position_size(10000.0, 1000.0, 0.05, 0.1);
        assert_eq!(size, 0.0);
        
        // At minimum spread
        let size = engine.calculate_position_size(10000.0, 1000.0, 0.1, 0.1);
        assert!(size > 0.0);
    }
}
