//! Jupiter Client
//!
//! Handles Jupiter DEX aggregator integration for spot swaps:
//! - Quote fetching with route optimization
//! - Swap instruction building
//! - Slippage management

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, info};

use crate::config::JupiterConfig;

/// Jupiter quote response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    pub input_mint: String,
    pub in_amount: String,
    pub output_mint: String,
    pub out_amount: String,
    pub other_amount_threshold: String,
    pub swap_mode: String,
    pub slippage_bps: u32,
    pub price_impact_pct: String,
    pub route_plan: Vec<RoutePlan>,
}

/// Route plan segment
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    pub swap_info: SwapInfo,
    pub percent: u8,
}

/// Swap info
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    pub amm_key: String,
    pub label: Option<String>,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: String,
    pub out_amount: String,
    pub fee_amount: String,
    pub fee_mint: String,
}

/// Swap request for Jupiter API
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapRequest {
    pub quote_response: serde_json::Value,
    pub user_public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_and_unwrap_sol: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_shared_accounts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_unit_price_micro_lamports: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_legacy_transaction: Option<bool>,
}

/// Swap response from Jupiter API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    pub swap_transaction: String,
    pub last_valid_block_height: u64,
}

/// Jupiter swap result
#[derive(Debug, Clone)]
pub struct SwapResult {
    pub input_amount: u64,
    pub output_amount: u64,
    pub min_output_amount: u64,
    pub price_impact_pct: f64,
    pub transaction_data: Vec<u8>,
}

/// Jupiter client for spot swaps
pub struct JupiterClient {
    /// HTTP client
    client: Client,
    /// Jupiter API URL
    api_url: String,
    /// SOL mint address
    sol_mint: Pubkey,
    /// USDC mint address
    usdc_mint: Pubkey,
}

impl JupiterClient {
    /// Create a new Jupiter client
    pub fn new(config: &JupiterConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;
        
        let sol_mint = Pubkey::from_str(&config.sol_mint)
            .context("Invalid SOL mint address")?;
        let usdc_mint = Pubkey::from_str(&config.usdc_mint)
            .context("Invalid USDC mint address")?;
        
        Ok(Self {
            client,
            api_url: config.api_url.clone(),
            sol_mint,
            usdc_mint,
        })
    }
    
    /// Get a quote for swapping tokens
    pub async fn get_quote(
        &self,
        input_mint: &Pubkey,
        output_mint: &Pubkey,
        amount: u64,
        slippage_bps: u16,
    ) -> Result<QuoteResponse> {
        let url = format!(
            "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
            self.api_url, input_mint, output_mint, amount, slippage_bps
        );
        
        debug!("Fetching Jupiter quote: {}", url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Jupiter quote")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Jupiter quote failed: {} - {}", status, body);
        }
        
        let quote: QuoteResponse = response.json().await
            .context("Failed to parse Jupiter quote")?;
        
        info!(
            "Jupiter quote: {} -> {}, price_impact: {}%",
            quote.in_amount, quote.out_amount, quote.price_impact_pct
        );
        
        Ok(quote)
    }
    
    /// Get quote for SOL -> USDC swap
    pub async fn get_sol_to_usdc_quote(
        &self,
        sol_amount_lamports: u64,
        slippage_bps: u16,
    ) -> Result<QuoteResponse> {
        self.get_quote(&self.sol_mint, &self.usdc_mint, sol_amount_lamports, slippage_bps).await
    }
    
    /// Get quote for USDC -> SOL swap
    pub async fn get_usdc_to_sol_quote(
        &self,
        usdc_amount: u64,
        slippage_bps: u16,
    ) -> Result<QuoteResponse> {
        self.get_quote(&self.usdc_mint, &self.sol_mint, usdc_amount, slippage_bps).await
    }
    
    /// Get swap transaction from quote
    pub async fn get_swap_transaction(
        &self,
        quote: &QuoteResponse,
        user_pubkey: &Pubkey,
        priority_fee: Option<u64>,
    ) -> Result<SwapResult> {
        let url = format!("{}/swap", self.api_url);
        
        let quote_json = serde_json::to_value(quote)
            .context("Failed to serialize quote")?;
        
        let request = SwapRequest {
            quote_response: quote_json,
            user_public_key: user_pubkey.to_string(),
            wrap_and_unwrap_sol: Some(true),
            use_shared_accounts: Some(true),
            compute_unit_price_micro_lamports: priority_fee,
            as_legacy_transaction: Some(false),
        };
        
        debug!("Fetching Jupiter swap transaction");
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to fetch Jupiter swap transaction")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Jupiter swap failed: {} - {}", status, body);
        }
        
        let swap_response: SwapResponse = response.json().await
            .context("Failed to parse Jupiter swap response")?;
        
        // Decode base64 transaction
        let transaction_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &swap_response.swap_transaction,
        ).context("Failed to decode swap transaction")?;
        
        let input_amount: u64 = quote.in_amount.parse().unwrap_or(0);
        let output_amount: u64 = quote.out_amount.parse().unwrap_or(0);
        let min_output_amount: u64 = quote.other_amount_threshold.parse().unwrap_or(0);
        let price_impact_pct: f64 = quote.price_impact_pct.parse().unwrap_or(0.0);
        
        info!(
            "Jupiter swap tx ready: {} -> {} (min: {}), impact: {:.4}%",
            input_amount, output_amount, min_output_amount, price_impact_pct
        );
        
        Ok(SwapResult {
            input_amount,
            output_amount,
            min_output_amount,
            price_impact_pct,
            transaction_data,
        })
    }
    
    /// Execute a complete SOL -> USDC swap quote and transaction fetch
    pub async fn prepare_sol_to_usdc_swap(
        &self,
        sol_amount_lamports: u64,
        slippage_bps: u16,
        user_pubkey: &Pubkey,
        priority_fee: Option<u64>,
    ) -> Result<SwapResult> {
        let quote = self.get_sol_to_usdc_quote(sol_amount_lamports, slippage_bps).await?;
        self.get_swap_transaction(&quote, user_pubkey, priority_fee).await
    }
    
    /// Get SOL mint
    pub fn sol_mint(&self) -> &Pubkey {
        &self.sol_mint
    }
    
    /// Get USDC mint
    pub fn usdc_mint(&self) -> &Pubkey {
        &self.usdc_mint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_response_parse() {
        let json = r#"{
            "inputMint": "So11111111111111111111111111111111111111112",
            "inAmount": "1000000000",
            "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "outAmount": "150000000",
            "otherAmountThreshold": "149250000",
            "swapMode": "ExactIn",
            "slippageBps": 50,
            "priceImpactPct": "0.01",
            "routePlan": []
        }"#;
        
        let quote: QuoteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(quote.in_amount, "1000000000");
        assert_eq!(quote.slippage_bps, 50);
    }
}
