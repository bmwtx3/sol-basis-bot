//! Funding Reversal Detection
//!
//! Early warning system for funding rate reversals:
//! - Velocity-based detection (rate of change)
//! - Pattern recognition (momentum shifts)
//! - Severity classification
//! - Alert generation with actionable recommendations

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::network::event_bus::Event;
use crate::state::SharedState;

/// Reversal severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReversalSeverity {
    /// Minor shift - monitor closely
    Low,
    /// Moderate reversal - consider reducing position
    Medium,
    /// Significant reversal - recommend closing
    High,
    /// Critical reversal - urgent action needed
    Critical,
}

impl ReversalSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
    
    pub fn score(&self) -> f64 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::Critical => 1.0,
        }
    }
}

/// Reversal alert
#[derive(Debug, Clone)]
pub struct ReversalAlert {
    /// Alert timestamp
    pub timestamp: i64,
    /// Severity level
    pub severity: ReversalSeverity,
    /// Current funding rate
    pub current_rate: f64,
    /// Current APR
    pub current_apr: f64,
    /// Funding velocity (rate of change per hour)
    pub velocity: f64,
    /// Acceleration (change in velocity)
    pub acceleration: f64,
    /// Predicted time to zero crossing (hours)
    pub time_to_zero_hours: Option<f64>,
    /// Predicted funding in 1 hour
    pub predicted_1h_apr: f64,
    /// Predicted funding in 8 hours
    pub predicted_8h_apr: f64,
    /// Recommendation
    pub recommendation: String,
    /// Detailed reasons
    pub reasons: Vec<String>,
    /// Confidence in prediction (0-1)
    pub confidence: f64,
}

/// Funding rate sample for history tracking
#[derive(Debug, Clone)]
struct FundingSample {
    timestamp: i64,
    rate: f64,
    apr: f64,
}

/// Reversal detector
pub struct ReversalDetector {
    /// Configuration
    config: Arc<AppConfig>,
    /// Shared state
    state: Arc<SharedState>,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Funding history (for velocity calculation)
    history: Arc<RwLock<VecDeque<FundingSample>>>,
    /// Last calculated velocity
    last_velocity: Arc<RwLock<f64>>,
    /// Last alert
    last_alert: Arc<RwLock<Option<ReversalAlert>>>,
    /// Alert history
    alert_history: Arc<RwLock<Vec<ReversalAlert>>>,
    /// Cooldown between alerts (ms)
    alert_cooldown_ms: i64,
    /// Last alert time
    last_alert_time: Arc<RwLock<i64>>,
}

impl ReversalDetector {
    /// Create a new reversal detector
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
            history: Arc::new(RwLock::new(VecDeque::with_capacity(480))), // 4 hours at 30s
            last_velocity: Arc::new(RwLock::new(0.0)),
            last_alert: Arc::new(RwLock::new(None)),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            alert_cooldown_ms: 5 * 60 * 1000, // 5 minutes between alerts
            last_alert_time: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Start the reversal detector
    pub async fn start(&self) -> anyhow::Result<()> {
        *self.running.write().await = true;
        info!("Reversal detector starting");
        
        let running = self.running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let event_tx = self.event_tx.clone();
        let history = self.history.clone();
        let last_velocity = self.last_velocity.clone();
        let last_alert = self.last_alert.clone();
        let alert_history = self.alert_history.clone();
        let alert_cooldown_ms = self.alert_cooldown_ms;
        let last_alert_time = self.last_alert_time.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            
            while *running.read().await {
                interval.tick().await;
                
                let current_rate = state.current_funding_rate.load();
                let current_apr = state.funding_apr.load();
                let timestamp = chrono::Utc::now().timestamp_millis();
                
                if current_rate.abs() < 0.000001 {
                    continue; // Skip if no funding data
                }
                
                // Add to history
                {
                    let mut hist = history.write().await;
                    hist.push_back(FundingSample {
                        timestamp,
                        rate: current_rate,
                        apr: current_apr,
                    });
                    
                    // Keep last 4 hours
                    let cutoff = timestamp - (4 * 60 * 60 * 1000);
                    while hist.front().map(|s| s.timestamp < cutoff).unwrap_or(false) {
                        hist.pop_front();
                    }
                }
                
                // Analyze for reversal
                let analysis = Self::analyze_reversal(
                    &history,
                    current_rate,
                    current_apr,
                    &config,
                    timestamp,
                ).await;
                
                // Store velocity
                *last_velocity.write().await = analysis.velocity;
                
                // Check if we should generate alert
                if let Some(alert) = analysis.alert {
                    let last_time = *last_alert_time.read().await;
                    
                    // Only alert if cooldown passed or severity increased
                    let should_alert = timestamp - last_time > alert_cooldown_ms
                        || last_alert.read().await.as_ref()
                            .map(|a| alert.severity.score() > a.severity.score())
                            .unwrap_or(true);
                    
                    if should_alert {
                        info!(
                            "🚨 Funding reversal alert: {} | APR: {:.1}% | Velocity: {:.4}/hr | {}",
                            alert.severity.as_str(),
                            alert.current_apr,
                            alert.velocity,
                            alert.recommendation
                        );
                        
                        // Store alert
                        *last_alert.write().await = Some(alert.clone());
                        *last_alert_time.write().await = timestamp;
                        
                        // Add to history
                        {
                            let mut hist = alert_history.write().await;
                            hist.push(alert.clone());
                            if hist.len() > 100 {
                                hist.remove(0);
                            }
                        }
                        
                        // Emit event
                        let _ = event_tx.send(Event::TradeSignal {
                            signal_type: format!("funding_reversal_{}", alert.severity.as_str().to_lowercase()),
                            size: 0.0,
                            reason: alert.recommendation.clone(),
                        });
                    }
                }
            }
            
            info!("Reversal detector stopped");
        });
        
        Ok(())
    }
    
    /// Analyze funding for reversal signals
    async fn analyze_reversal(
        history: &Arc<RwLock<VecDeque<FundingSample>>>,
        current_rate: f64,
        current_apr: f64,
        config: &AppConfig,
        timestamp: i64,
    ) -> ReversalAnalysis {
        let hist = history.read().await;
        
        if hist.len() < 10 {
            return ReversalAnalysis {
                velocity: 0.0,
                acceleration: 0.0,
                alert: None,
            };
        }
        
        // Calculate velocity (rate of change per hour)
        let velocity = Self::calculate_velocity(&hist);
        
        // Calculate acceleration (change in velocity)
        let acceleration = Self::calculate_acceleration(&hist);
        
        // Check for reversal conditions
        let is_positive = current_rate > 0.0;
        let is_reversing = (is_positive && velocity < 0.0) || (!is_positive && velocity > 0.0);
        
        if !is_reversing {
            return ReversalAnalysis {
                velocity,
                acceleration,
                alert: None,
            };
        }
        
        // Calculate reversal metrics
        let velocity_magnitude = velocity.abs();
        let acceleration_magnitude = acceleration.abs();
        
        // Predict time to zero crossing
        let time_to_zero = if velocity_magnitude > 0.0001 {
            Some(current_rate.abs() / velocity_magnitude)
        } else {
            None
        };
        
        // Predict future funding
        let predicted_1h = current_apr + (velocity * 1.0 * 24.0 * 365.0 * 100.0);
        let predicted_8h = current_apr + (velocity * 8.0 * 24.0 * 365.0 * 100.0);
        
        // Determine severity
        let severity = Self::determine_severity(
            velocity_magnitude,
            acceleration_magnitude,
            current_apr.abs(),
            time_to_zero,
            config,
        );
        
        // Build reasons
        let mut reasons = Vec::new();
        
        reasons.push(format!(
            "Funding {} at {:.4}/hr",
            if is_positive { "decreasing" } else { "increasing" },
            velocity_magnitude
        ));
        
        if acceleration_magnitude > 0.00001 {
            let acc_direction = if (is_positive && acceleration < 0.0) || (!is_positive && acceleration > 0.0) {
                "accelerating"
            } else {
                "decelerating"
            };
            reasons.push(format!("Reversal {} (acc: {:.6})", acc_direction, acceleration));
        }
        
        if let Some(ttz) = time_to_zero {
            if ttz < 24.0 {
                reasons.push(format!("Zero crossing in ~{:.1} hours", ttz));
            }
        }
        
        if predicted_8h.signum() != current_apr.signum() {
            reasons.push("Predicted sign flip within 8 hours".to_string());
        }
        
        // Generate recommendation
        let recommendation = Self::generate_recommendation(
            severity,
            current_apr,
            predicted_8h,
            time_to_zero,
        );
        
        // Calculate confidence
        let confidence = Self::calculate_confidence(&hist, velocity_magnitude, acceleration_magnitude);
        
        let alert = ReversalAlert {
            timestamp,
            severity,
            current_rate,
            current_apr,
            velocity,
            acceleration,
            time_to_zero_hours: time_to_zero,
            predicted_1h_apr: predicted_1h,
            predicted_8h_apr: predicted_8h,
            recommendation,
            reasons,
            confidence,
        };
        
        ReversalAnalysis {
            velocity,
            acceleration,
            alert: Some(alert),
        }
    }
    
    /// Calculate velocity (rate of change per hour)
    fn calculate_velocity(history: &VecDeque<FundingSample>) -> f64 {
        if history.len() < 2 {
            return 0.0;
        }
        
        // Use last 30 minutes of data for velocity
        let cutoff = history.back().map(|s| s.timestamp - 30 * 60 * 1000).unwrap_or(0);
        let recent: Vec<_> = history.iter().filter(|s| s.timestamp >= cutoff).collect();
        
        if recent.len() < 2 {
            return 0.0;
        }
        
        // Linear regression for more stable velocity
        let n = recent.len() as f64;
        let sum_x: f64 = recent.iter().enumerate().map(|(i, _)| i as f64).sum();
        let sum_y: f64 = recent.iter().map(|s| s.rate).sum();
        let sum_xy: f64 = recent.iter().enumerate().map(|(i, s)| i as f64 * s.rate).sum();
        let sum_xx: f64 = recent.iter().enumerate().map(|(i, _)| (i as f64).powi(2)).sum();
        
        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x.powi(2));
        
        // Convert to per-hour (samples are 30s apart)
        slope * 120.0 // 120 samples per hour
    }
    
    /// Calculate acceleration (change in velocity)
    fn calculate_acceleration(history: &VecDeque<FundingSample>) -> f64 {
        if history.len() < 20 {
            return 0.0;
        }
        
        // Split into two halves and compare velocities
        let mid = history.len() / 2;
        
        let first_half: VecDeque<_> = history.iter().take(mid).cloned().collect();
        let second_half: VecDeque<_> = history.iter().skip(mid).cloned().collect();
        
        let v1 = Self::calculate_velocity(&first_half);
        let v2 = Self::calculate_velocity(&second_half);
        
        // Time between midpoints (rough estimate)
        let time_hours = (history.len() as f64 * 30.0) / 3600.0 / 2.0;
        
        if time_hours > 0.0 {
            (v2 - v1) / time_hours
        } else {
            0.0
        }
    }
    
    /// Determine severity of reversal
    fn determine_severity(
        velocity_magnitude: f64,
        acceleration_magnitude: f64,
        current_apr_magnitude: f64,
        time_to_zero: Option<f64>,
        _config: &AppConfig,
    ) -> ReversalSeverity {
        // Critical: fast reversal with zero crossing imminent
        if let Some(ttz) = time_to_zero {
            if ttz < 4.0 && velocity_magnitude > 0.0001 {
                return ReversalSeverity::Critical;
            }
        }
        
        // High: significant velocity against position
        if velocity_magnitude > 0.0002 || (velocity_magnitude > 0.0001 && acceleration_magnitude > 0.00005) {
            return ReversalSeverity::High;
        }
        
        // Medium: moderate reversal
        if velocity_magnitude > 0.00005 && time_to_zero.map(|t| t < 12.0).unwrap_or(false) {
            return ReversalSeverity::Medium;
        }
        
        // Low: early warning
        if velocity_magnitude > 0.00002 {
            return ReversalSeverity::Low;
        }
        
        // Default to low if we got here (some reversal detected)
        ReversalSeverity::Low
    }
    
    /// Generate recommendation based on severity
    fn generate_recommendation(
        severity: ReversalSeverity,
        current_apr: f64,
        predicted_8h: f64,
        time_to_zero: Option<f64>,
    ) -> String {
        match severity {
            ReversalSeverity::Critical => {
                if let Some(ttz) = time_to_zero {
                    format!(
                        "URGENT: Close position immediately. Funding reversal in ~{:.1}h. \
                         Current: {:.1}% → Predicted: {:.1}%",
                        ttz, current_apr, predicted_8h
                    )
                } else {
                    format!(
                        "URGENT: Close position immediately. Rapid funding reversal detected. \
                         Current: {:.1}% → Predicted: {:.1}%",
                        current_apr, predicted_8h
                    )
                }
            }
            ReversalSeverity::High => {
                format!(
                    "RECOMMENDED: Reduce or close position. Significant funding reversal. \
                     Current: {:.1}% → Predicted 8h: {:.1}%",
                    current_apr, predicted_8h
                )
            }
            ReversalSeverity::Medium => {
                format!(
                    "CAUTION: Monitor closely. Funding momentum shifting. \
                     Current: {:.1}% → Predicted 8h: {:.1}%",
                    current_apr, predicted_8h
                )
            }
            ReversalSeverity::Low => {
                format!(
                    "NOTICE: Early reversal signal detected. \
                     Current: {:.1}% → Predicted 8h: {:.1}%",
                    current_apr, predicted_8h
                )
            }
        }
    }
    
    /// Calculate confidence in prediction
    fn calculate_confidence(
        history: &VecDeque<FundingSample>,
        velocity_magnitude: f64,
        acceleration_magnitude: f64,
    ) -> f64 {
        let mut confidence = 0.5; // Base confidence
        
        // More data = higher confidence
        let data_factor = (history.len() as f64 / 100.0).min(1.0);
        confidence += data_factor * 0.2;
        
        // Strong, consistent velocity = higher confidence
        if velocity_magnitude > 0.0001 {
            confidence += 0.15;
        }
        
        // Accelerating reversal = higher confidence
        if acceleration_magnitude > 0.00002 {
            confidence += 0.1;
        }
        
        confidence.min(0.95)
    }
    
    /// Stop the reversal detector
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Reversal detector stopping");
    }
    
    /// Get current velocity
    pub async fn get_velocity(&self) -> f64 {
        *self.last_velocity.read().await
    }
    
    /// Get last alert
    pub async fn get_last_alert(&self) -> Option<ReversalAlert> {
        self.last_alert.read().await.clone()
    }
    
    /// Get alert history
    pub async fn get_alert_history(&self) -> Vec<ReversalAlert> {
        self.alert_history.read().await.clone()
    }
    
    /// Check if reversal is active
    pub async fn is_reversal_active(&self) -> bool {
        self.last_alert.read().await.as_ref()
            .map(|a| {
                let now = chrono::Utc::now().timestamp_millis();
                // Consider reversal active if alert within last 30 minutes
                now - a.timestamp < 30 * 60 * 1000
            })
            .unwrap_or(false)
    }
    
    /// Get reversal severity (if active)
    pub async fn get_reversal_severity(&self) -> Option<ReversalSeverity> {
        if self.is_reversal_active().await {
            self.last_alert.read().await.as_ref().map(|a| a.severity)
        } else {
            None
        }
    }
    
    /// Manual check for reversal (for testing)
    pub async fn check_now(&self) -> Option<ReversalAlert> {
        let current_rate = self.state.current_funding_rate.load();
        let current_apr = self.state.funding_apr.load();
        let timestamp = chrono::Utc::now().timestamp_millis();
        
        let analysis = Self::analyze_reversal(
            &self.history,
            current_rate,
            current_apr,
            &self.config,
            timestamp,
        ).await;
        
        analysis.alert
    }
}

/// Internal analysis result
struct ReversalAnalysis {
    velocity: f64,
    acceleration: f64,
    alert: Option<ReversalAlert>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(ReversalSeverity::Critical.score() > ReversalSeverity::High.score());
        assert!(ReversalSeverity::High.score() > ReversalSeverity::Medium.score());
        assert!(ReversalSeverity::Medium.score() > ReversalSeverity::Low.score());
    }

    #[test]
    fn test_velocity_calculation() {
        let mut history = VecDeque::new();
        let now = 1000000;
        
        // Add samples with decreasing rate
        for i in 0..20 {
            history.push_back(FundingSample {
                timestamp: now + i * 30000,
                rate: 0.001 - (i as f64 * 0.00005),
                apr: 0.0,
            });
        }
        
        let velocity = ReversalDetector::calculate_velocity(&history);
        assert!(velocity < 0.0, "Velocity should be negative for decreasing rate");
    }
}
