//! Pyth Oracle Price Feed
//!
//! Fetches SOL/USD price from Pyth Network oracle.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::PythConfig;
use crate::network::event_bus::Event;
use crate::utils::types::{PriceSource, PriceUpdate};

/// Pyth price feed
pub struct PythFeed {
    /// SOL/USD feed address
    feed_address: String,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Last price
    last_price: Arc<RwLock<Option<f64>>>,
    /// HTTP client
    client: reqwest::Client,
}

impl PythFeed {
    /// Create a new Pyth feed
    pub fn new(config: &PythConfig, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            feed_address: config.sol_usd_feed.clone(),
            event_tx,
            running: Arc::new(RwLock::new(false)),
            last_price: Arc::new(RwLock::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }
    
    /// Start the price feed
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Pyth price feed starting for {}", self.feed_address);
        
        let running = self.running.clone();
        let feed_address = self.feed_address.clone();
        let event_tx = self.event_tx.clone();
        let last_price = self.last_price.clone();
        let client = self.client.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            
            while *running.read().await {
                interval.tick().await;
                
                match Self::fetch_price(&client, &feed_address).await {
                    Ok(price) => {
                        debug!("Pyth SOL/USD price: ${:.4}", price);
                        
                        *last_price.write().await = Some(price);
                        
                        let update = PriceUpdate {
                            source: PriceSource::Pyth,
                            price,
                            confidence: None,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        };
                        
                        let _ = event_tx.send(Event::SpotPriceUpdate(update));
                    }
                    Err(e) => {
                        warn!("Failed to fetch Pyth price: {}", e);
                    }
                }
            }
            
            info!("Pyth price feed stopped");
        });
        
        Ok(())
    }
    
    /// Fetch price from Pyth Hermes API
    async fn fetch_price(client: &reqwest::Client, feed_id: &str) -> Result<f64> {
        // Use Pyth Hermes API for real-time prices
        let url = format!(
            "https://hermes.pyth.network/api/latest_price_feeds?ids[]={}",
            feed_id
        );
        
        let response = client
            .get(&url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        // Parse the response
        if let Some(feeds) = response.as_array() {
            if let Some(feed) = feeds.first() {
                if let Some(price_obj) = feed.get("price") {
                    if let (Some(price_str), Some(expo)) = (
                        price_obj.get("price").and_then(|p| p.as_str()),
                        price_obj.get("expo").and_then(|e| e.as_i64()),
                    ) {
                        let price: i64 = price_str.parse()?;
                        let price_f64 = price as f64 * 10_f64.powi(expo as i32);
                        return Ok(price_f64);
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("Failed to parse Pyth price response"))
    }
    
    /// Stop the price feed
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Pyth price feed stopping");
    }
    
    /// Get last known price
    pub async fn get_last_price(&self) -> Option<f64> {
        *self.last_price.read().await
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
    fn test_pyth_feed_creation() {
        let config = PythConfig {
            sol_usd_feed: "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG".to_string(),
        };
        let (tx, _) = broadcast::channel(10);
        let feed = PythFeed::new(&config, tx);
        assert_eq!(feed.feed_address, config.sol_usd_feed);
    }
}
