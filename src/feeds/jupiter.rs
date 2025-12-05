//! Jupiter Spot Price Feed
//!
//! Fetches aggregated spot prices from Jupiter.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::JupiterConfig;
use crate::network::event_bus::Event;
use crate::utils::types::{PriceSource, PriceUpdate};

/// Jupiter price response
#[derive(Debug, Serialize, Deserialize)]
pub struct JupiterPriceResponse {
    pub data: std::collections::HashMap<String, JupiterPriceData>,
    #[serde(rename = "timeTaken")]
    pub time_taken: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JupiterPriceData {
    pub id: String,
    #[serde(rename = "mintSymbol")]
    pub mint_symbol: Option<String>,
    #[serde(rename = "vsToken")]
    pub vs_token: Option<String>,
    #[serde(rename = "vsTokenSymbol")]
    pub vs_token_symbol: Option<String>,
    pub price: f64,
}

/// Jupiter price feed
pub struct JupiterFeed {
    /// API URL
    api_url: String,
    /// SOL mint address
    sol_mint: String,
    /// USDC mint address
    usdc_mint: String,
    /// Event sender
    event_tx: broadcast::Sender<Event>,
    /// Is running
    running: Arc<RwLock<bool>>,
    /// Last price
    last_price: Arc<RwLock<Option<f64>>>,
    /// HTTP client
    client: reqwest::Client,
}

impl JupiterFeed {
    /// Create a new Jupiter feed
    pub fn new(config: &JupiterConfig, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            api_url: config.api_url.clone(),
            sol_mint: config.sol_mint.clone(),
            usdc_mint: config.usdc_mint.clone(),
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
        info!("Jupiter price feed starting");
        
        let running = self.running.clone();
        let sol_mint = self.sol_mint.clone();
        let event_tx = self.event_tx.clone();
        let last_price = self.last_price.clone();
        let client = self.client.clone();
        
        tokio::spawn(async move {
            // Poll every 1 second (Jupiter has rate limits)
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            while *running.read().await {
                interval.tick().await;
                
                match Self::fetch_price(&client, &sol_mint).await {
                    Ok(price) => {
                        debug!("Jupiter SOL/USDC price: ${:.4}", price);
                        
                        *last_price.write().await = Some(price);
                        
                        let update = PriceUpdate {
                            source: PriceSource::Jupiter,
                            price,
                            confidence: None,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        };
                        
                        // Jupiter provides spot price backup/validation
                        let _ = event_tx.send(Event::SpotPriceUpdate(update));
                    }
                    Err(e) => {
                        warn!("Failed to fetch Jupiter price: {}", e);
                    }
                }
            }
            
            info!("Jupiter price feed stopped");
        });
        
        Ok(())
    }
    
    /// Fetch price from Jupiter Price API
    async fn fetch_price(client: &reqwest::Client, sol_mint: &str) -> Result<f64> {
        let url = format!(
            "https://price.jup.ag/v6/price?ids={}",
            sol_mint
        );
        
        let response: JupiterPriceResponse = client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        response
            .data
            .get(sol_mint)
            .map(|d| d.price)
            .ok_or_else(|| anyhow::anyhow!("SOL price not found in Jupiter response"))
    }
    
    /// Stop the price feed
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Jupiter price feed stopping");
    }
    
    /// Get last known price
    pub async fn get_last_price(&self) -> Option<f64> {
        *self.last_price.read().await
    }
    
    /// Check if feed is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
    
    /// Get quote for a swap
    pub async fn get_quote(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: u64,
    ) -> Result<JupiterQuote> {
        let url = format!(
            "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps=50",
            self.api_url, input_mint, output_mint, amount
        );
        
        let quote: JupiterQuote = self.client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        Ok(quote)
    }
}

/// Jupiter quote response
#[derive(Debug, Serialize, Deserialize)]
pub struct JupiterQuote {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "otherAmountThreshold")]
    pub other_amount_threshold: String,
    #[serde(rename = "swapMode")]
    pub swap_mode: String,
    #[serde(rename = "slippageBps")]
    pub slippage_bps: u32,
    #[serde(rename = "priceImpactPct")]
    pub price_impact_pct: String,
    #[serde(rename = "routePlan")]
    pub route_plan: Vec<RoutePlan>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RoutePlan {
    #[serde(rename = "swapInfo")]
    pub swap_info: SwapInfo,
    pub percent: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapInfo {
    #[serde(rename = "ammKey")]
    pub amm_key: String,
    pub label: Option<String>,
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "feeAmount")]
    pub fee_amount: String,
    #[serde(rename = "feeMint")]
    pub fee_mint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jupiter_feed_creation() {
        let config = JupiterConfig {
            api_url: "https://quote-api.jup.ag/v6".to_string(),
            sol_mint: "So11111111111111111111111111111111111111112".to_string(),
            usdc_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        };
        let (tx, _) = broadcast::channel(10);
        let feed = JupiterFeed::new(&config, tx);
        assert_eq!(feed.sol_mint, config.sol_mint);
    }
}
