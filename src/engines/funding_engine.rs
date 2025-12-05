//! Funding Rate Engine
//!
//! Analyzes funding rates with:
//! - 8-hour rolling window tracking
//! - Annualized APR calculation
//! - Funding velocity (rate of change)
//! - Predicted next funding payment
//! - Volatility detection

use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;

/// Funding rate snapshot for history
#[derive(Debug, Clone)]
pub struct FundingRateSnapshot {
    pub timestamp: i64,
    pub rate: f64,
    pub apr: f64,
}

/// Funding analysis result
#[derive(Debug, Clone)]
pub struct FundingAnalysis {
    /// Current hourly funding rate
    pub current_rate: f64,
    /// Annualized APR (current_rate * 24 * 365 * 100)
    pub annualized_apr: f64,
    /// Average rate over last 8 hours
    pub avg_8h_rate: f64,
    /// Average APR over last 8 hours
    pub avg_8h_apr: f64,
    /// Funding velocity (rate of change per hour)
    pub velocity: f64,
    /// Predicted next funding payment (in USD per $1000 position)
    pub predicted_payment: f64,
    /// Volatility of funding rate
    pub volatility: f64,
    /// Is funding rate elevated (above threshold)
    pub is_elevated: bool,
    /// Is funding rate reversing direction
    pub is_reversing: bool,
    /// Timestamp
    pub timestamp: i64,
}

/// Funding rate engine
pub struct FundingEngine {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Funding history (8-hour rolling window)
    history: Arc<RwLock<VecDeque<FundingRateSnapshot>>>,
    /// Last analysis result
    last_analysis: Arc<RwLock<Option<FundingAnalysis>>>,
}

impl FundingEngine {
    /// Create a new funding engine
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
            history: Arc::new(RwLock::new(VecDeque::with_capacity(960))), // 8 hours at 30s intervals
            last_analysis: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the funding engine
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Funding engine starting");
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let event_tx = self.event_tx.clone();
        let history = self.history.clone();
        let last_analysis = self.last_analysis.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            while *running.read().await {
                interval.tick().await;
                
                // Get current funding rate from state
                let current_rate = state.current_funding_rate.load();
                let current_apr = state.funding_apr.load();
                let timestamp = chrono::Utc::now().timestamp_millis();
                
                if current_rate.abs() > 0.0 {
                    // Add to history
                    {
                        let mut hist = history.write().await;
                        hist.push_back(FundingRateSnapshot {
                            timestamp,
                            rate: current_rate,
                            apr: current_apr,
                        });
                        
                        // Keep only last 8 hours (960 samples at 30s intervals)
                        let cutoff = timestamp - (8 * 60 * 60 * 1000);
                        while hist.front().map(|s| s.timestamp < cutoff).unwrap_or(false) {
                            hist.pop_front();
                        }
                    }
                    
                    // Perform analysis
                    let analysis = Self::analyze(
                        &history,
                        current_rate,
                        current_apr,
                        config.trading.min_funding_apr_pct,
                        timestamp,
                    ).await;
                    
                    debug!(
                        "Funding analysis: APR={:.2}%, 8h_avg={:.2}%, velocity={:.4}, vol={:.4}",
                        analysis.annualized_apr,
                        analysis.avg_8h_apr,
                        analysis.velocity,
                        analysis.volatility
                    );
                    
                    // Store analysis
                    *last_analysis.write().await = Some(analysis.clone());
                    
                    // Emit events for significant changes
                    if analysis.is_elevated {
                        let _ = event_tx.send(Event::TradeSignal {
                            signal_type: "funding_elevated".to_string(),
                            size: 0.0,
                            reason: format!(
                                "Funding APR {:.2}% exceeds threshold {:.2}%",
                                analysis.annualized_apr,
                                config.trading.min_funding_apr_pct
                            ),
                        });
                    }
                    
                    if analysis.is_reversing {
                        let _ = event_tx.send(Event::TradeSignal {
                            signal_type: "funding_reversing".to_string(),
                            size: 0.0,
                            reason: format!(
                                "Funding rate reversing: velocity={:.6}",
                                analysis.velocity
                            ),
                        });
                    }
                }
            }
            
            info!("Funding engine stopped");
        });
        
        Ok(())
    }
    
    /// Analyze funding rates
    async fn analyze(
        history: &Arc<RwLock<VecDeque<FundingRateSnapshot>>>,
        current_rate: f64,
        current_apr: f64,
        threshold_apr: f64,
        timestamp: i64,
    ) -> FundingAnalysis {
        let hist = history.read().await;
        
        // Calculate averages
        let (avg_rate, avg_apr) = if hist.is_empty() {
            (current_rate, current_apr)
        } else {
            let sum_rate: f64 = hist.iter().map(|s| s.rate).sum();
            let sum_apr: f64 = hist.iter().map(|s| s.apr).sum();
            let count = hist.len() as f64;
            (sum_rate / count, sum_apr / count)
        };
        
        // Calculate velocity (rate of change)
        let velocity = if hist.len() >= 2 {
            let recent: Vec<_> = hist.iter().rev().take(10).collect();
            if recent.len() >= 2 {
                let first = recent.last().unwrap();
                let last = recent.first().unwrap();
                let time_diff = (last.timestamp - first.timestamp) as f64 / 3600000.0; // hours
                if time_diff > 0.0 {
                    (last.rate - first.rate) / time_diff
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        // Calculate volatility (standard deviation)
        let volatility = if hist.len() >= 2 {
            let mean = avg_rate;
            let variance: f64 = hist.iter()
                .map(|s| (s.rate - mean).powi(2))
                .sum::<f64>() / hist.len() as f64;
            variance.sqrt()
        } else {
            0.0
        };
        
        // Predict next funding payment (per $1000 position)
        let predicted_payment = current_rate * 1000.0;
        
        // Check if elevated
        let is_elevated = current_apr.abs() >= threshold_apr;
        
        // Check if reversing (velocity opposing current direction)
        let is_reversing = (current_rate > 0.0 && velocity < -0.0001) ||
                          (current_rate < 0.0 && velocity > 0.0001);
        
        FundingAnalysis {
            current_rate,
            annualized_apr: current_apr,
            avg_8h_rate: avg_rate,
            avg_8h_apr: avg_apr,
            velocity,
            predicted_payment,
            volatility,
            is_elevated,
            is_reversing,
            timestamp,
        }
    }
    
    /// Stop the funding engine
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Funding engine stopping");
    }
    
    /// Get last analysis
    pub async fn get_last_analysis(&self) -> Option<FundingAnalysis> {
        self.last_analysis.read().await.clone()
    }
    
    /// Get 8-hour average APR
    pub async fn get_avg_8h_apr(&self) -> f64 {
        self.last_analysis.read().await
            .as_ref()
            .map(|a| a.avg_8h_apr)
            .unwrap_or(0.0)
    }
    
    /// Check if funding is elevated
    pub async fn is_funding_elevated(&self) -> bool {
        self.last_analysis.read().await
            .as_ref()
            .map(|a| a.is_elevated)
            .unwrap_or(false)
    }
    
    /// Get funding velocity
    pub async fn get_velocity(&self) -> f64 {
        self.last_analysis.read().await
            .as_ref()
            .map(|a| a.velocity)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_funding_analysis() {
        let snapshot = FundingRateSnapshot {
            timestamp: 1000,
            rate: 0.0001,
            apr: 8.76,
        };
        assert!(snapshot.rate > 0.0);
    }
}
