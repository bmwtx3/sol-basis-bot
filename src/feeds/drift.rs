//! Drift Protocol Price Feed
//!
//! Fetches perp market data from Drift Protocol including
//! mark price, index price, and funding rates.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::DriftConfig;
use crate::network::event_bus::Event;
use crate::utils::types::{PriceSource, PriceUpdate};

/// Drift market data response
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftMarketData {
    #[serde(rename = "marketIndex")]
    pub market_index: u16,
    #[serde(rename = "marketName")]
    pub market_name: Option<String>,
    #[serde(rename = "markPrice")]
    pub mark_price: Option<String>,
    #[serde(rename = "indexPrice")]
    pub index_price: Option<String>,
    #[serde(rename = "fundingRate")]
    pub funding_rate: Option<String>,
    #[serde(rename = "fundingRateLong")]
    pub funding_rate_long: Option<String>,
    #[serde(rename = "fundingRateShort")]
    pub funding_rate_short: Option<String>,
    #[serde(rename = "openInterest")]
    pub open_interest: Option<String>,
    #[serde(rename = "volume24h")]
    pub volume_24h: Option<String>,
}

/// Drift API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct DriftApiResponse {
    pub success: bool,
    pub data: Option<DriftMarketData>,
    pub error: Option<String>,
}

/// Drift price feed
pub struct DriftFeed {
    /// Program ID
    program_id: String,
    /// Market index (0 = SOL-PERP)
    market_index: u16,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Last mark price
    last_mark_price: Arc<RwLock<Option<f64>>>,
    /// Last index price
    last_index_price: Arc<RwLock<Option<f64>>>,
    /// Last funding rate
    last_funding_rate: Arc<RwLock<Option<f64>>>,
    /// HTTP client
    client: reqwest::Client,
}

impl DriftFeed {
    /// Create a new Drift feed
    pub fn new(config: &DriftConfig, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            program_id: config.program_id.clone(),
            market_index: config.market_index,
            event_tx,
            running: Arc::new(RwLock::new(false)),
            last_mark_price: Arc::new(RwLock::new(None)),
            last_index_price: Arc::new(RwLock::new(None)),
            last_funding_rate: Arc::new(RwLock::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }
    
    /// Start the price feed
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Drift price feed starting for market index {}", self.market_index);
        
        let running = self.running.clone();
        let market_index = self.market_index;
        let event_tx = self.event_tx.clone();
        let last_mark_price = self.last_mark_price.clone();
        let last_index_price = self.last_index_price.clone();
        let last_funding_rate = self.last_funding_rate.clone();
        let client = self.client.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            
            while *running.read().await {
                interval.tick().await;
                
                match Self::fetch_market_data(&client, market_index).await {
                    Ok(data) => {
                        // Parse mark price
                        if let Some(mark_str) = &data.mark_price {
                            if let Ok(mark) = mark_str.parse::<f64>() {
                                debug!("Drift SOL-PERP mark price: ${:.4}", mark);
                                *last_mark_price.write().await = Some(mark);
                                
                                let update = PriceUpdate {
                                    source: PriceSource::DriftMark,
                                    price: mark,
                                    confidence: None,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                };
                                let _ = event_tx.send(Event::PerpMarkPriceUpdate(update));
                            }
                        }
                        
                        // Parse index price
                        if let Some(index_str) = &data.index_price {
                            if let Ok(index) = index_str.parse::<f64>() {
                                debug!("Drift SOL-PERP index price: ${:.4}", index);
                                *last_index_price.write().await = Some(index);
                                
                                let update = PriceUpdate {
                                    source: PriceSource::DriftIndex,
                                    price: index,
                                    confidence: None,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                };
                                let _ = event_tx.send(Event::PerpIndexPriceUpdate(update));
                            }
                        }
                        
                        // Parse funding rate
                        if let Some(rate_str) = &data.funding_rate {
                            if let Ok(rate) = rate_str.parse::<f64>() {
                                debug!("Drift SOL-PERP funding rate: {:.6}%", rate * 100.0);
                                *last_funding_rate.write().await = Some(rate);
                                
                                let _ = event_tx.send(Event::FundingRateUpdate {
                                    rate,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch Drift market data: {}", e);
                    }
                }
            }
            
            info!("Drift price feed stopped");
        });
        
        Ok(())
    }
    
    /// Fetch market data from Drift API
    async fn fetch_market_data(
        client: &reqwest::Client,
        market_index: u16,
    ) -> Result<DriftMarketData> {
        // Use Drift's public API endpoint
        let url = format!(
            "https://mainnet-beta.api.drift.trade/stats/perpMarket?marketIndex={}",
            market_index
        );
        
        let response = client
            .get(&url)
            .send()
            .await?;
        
        // First try to parse as the API response
        let text = response.text().await?;
        
        // Try parsing as DriftApiResponse first
        if let Ok(api_response) = serde_json::from_str::<DriftApiResponse>(&text) {
            if api_response.success {
                if let Some(data) = api_response.data {
                    return Ok(data);
                }
            }
            if let Some(error) = api_response.error {
                return Err(anyhow::anyhow!("Drift API error: {}", error));
            }
        }
        
        // Try parsing directly as DriftMarketData
        if let Ok(data) = serde_json::from_str::<DriftMarketData>(&text) {
            return Ok(data);
        }
        
        Err(anyhow::anyhow!("Failed to parse Drift response: {}", text))
    }
    
    /// Stop the price feed
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Drift price feed stopping");
    }
    
    /// Get last mark price
    pub async fn get_last_mark_price(&self) -> Option<f64> {
        *self.last_mark_price.read().await
    }
    
    /// Get last index price
    pub async fn get_last_index_price(&self) -> Option<f64> {
        *self.last_index_price.read().await
    }
    
    /// Get last funding rate
    pub async fn get_last_funding_rate(&self) -> Option<f64> {
        *self.last_funding_rate.read().await
    }
    
    /// Check if feed is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_feed_creation() {
        let config = DriftConfig {
            program_id: "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH".to_string(),
            market_index: 0,
        };
        let (tx, _) = broadcast::channel(10);
        let feed = DriftFeed::new(&config, tx);
        assert_eq!(feed.market_index, 0);
    }
}
