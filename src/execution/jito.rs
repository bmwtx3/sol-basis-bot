//! Jito Client
//!
//! Handles Jito bundle submission for MEV protection:
//! - Bundle creation with tip
//! - Block engine submission
//! - Bundle status tracking

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::ExecutionConfig;

/// Jito tip accounts (rotated periodically)
const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Bundle status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleStatus {
    Pending,
    Landed,
    Failed(String),
    Expired,
}

/// Bundle submission response
#[derive(Debug, Clone, Deserialize)]
pub struct BundleResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub error: Option<JitoError>,
}

/// Jito error
#[derive(Debug, Clone, Deserialize)]
pub struct JitoError {
    pub code: i64,
    pub message: String,
}

/// Bundle status response
#[derive(Debug, Clone, Deserialize)]
pub struct BundleStatusResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default)]
    pub result: Option<BundleStatusResult>,
}

/// Bundle status result
#[derive(Debug, Clone, Deserialize)]
pub struct BundleStatusResult {
    pub context: StatusContext,
    pub value: Vec<BundleStatusValue>,
}

/// Status context
#[derive(Debug, Clone, Deserialize)]
pub struct StatusContext {
    pub slot: u64,
}

/// Bundle status value
#[derive(Debug, Clone, Deserialize)]
pub struct BundleStatusValue {
    pub bundle_id: String,
    pub status: String,
    #[serde(default)]
    pub landed_slot: Option<u64>,
}

/// Jito client for bundle submission
pub struct JitoClient {
    /// HTTP client
    client: Client,
    /// Block engine URL
    block_engine_url: String,
    /// Tip amount in lamports
    tip_lamports: u64,
    /// Current tip account index
    tip_account_index: std::sync::atomic::AtomicUsize,
}

impl JitoClient {
    /// Create a new Jito client
    pub fn new(config: &ExecutionConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;
        
        Ok(Self {
            client,
            block_engine_url: config.jito_block_engine_url.clone(),
            tip_lamports: config.jito_tip_lamports,
            tip_account_index: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    
    /// Get current tip account
    pub fn get_tip_account(&self) -> Pubkey {
        let index = self.tip_account_index.load(std::sync::atomic::Ordering::Relaxed);
        Pubkey::from_str(JITO_TIP_ACCOUNTS[index % JITO_TIP_ACCOUNTS.len()]).unwrap()
    }
    
    /// Rotate to next tip account
    pub fn rotate_tip_account(&self) {
        self.tip_account_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    /// Get tip amount
    pub fn tip_lamports(&self) -> u64 {
        self.tip_lamports
    }
    
    /// Submit a bundle of transactions
    pub async fn submit_bundle(&self, transactions: Vec<Transaction>) -> Result<String> {
        if transactions.is_empty() {
            anyhow::bail!("Cannot submit empty bundle");
        }
        
        // Serialize transactions to base64
        let encoded_txs: Vec<String> = transactions
            .iter()
            .map(|tx| {
                let serialized = bincode::serialize(tx)
                    .expect("Failed to serialize transaction");
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &serialized)
            })
            .collect();
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [encoded_txs]
        });
        
        debug!("Submitting Jito bundle with {} transactions", transactions.len());
        
        let response = self.client
            .post(&format!("{}/api/v1/bundles", self.block_engine_url))
            .json(&request)
            .send()
            .await
            .context("Failed to submit Jito bundle")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Jito bundle submission failed: {} - {}", status, body);
        }
        
        let bundle_response: BundleResponse = response.json().await
            .context("Failed to parse Jito bundle response")?;
        
        if let Some(error) = bundle_response.error {
            anyhow::bail!("Jito bundle error: {} - {}", error.code, error.message);
        }
        
        let bundle_id = bundle_response.result
            .ok_or_else(|| anyhow::anyhow!("No bundle ID returned"))?;
        
        info!("Jito bundle submitted: {}", bundle_id);
        
        // Rotate tip account for next submission
        self.rotate_tip_account();
        
        Ok(bundle_id)
    }
    
    /// Check bundle status
    pub async fn get_bundle_status(&self, bundle_id: &str) -> Result<BundleStatus> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBundleStatuses",
            "params": [[bundle_id]]
        });
        
        let response = self.client
            .post(&format!("{}/api/v1/bundles", self.block_engine_url))
            .json(&request)
            .send()
            .await
            .context("Failed to get bundle status")?;
        
        if !response.status().is_success() {
            return Ok(BundleStatus::Pending);
        }
        
        let status_response: BundleStatusResponse = response.json().await
            .context("Failed to parse bundle status response")?;
        
        if let Some(result) = status_response.result {
            if let Some(status_value) = result.value.first() {
                return match status_value.status.as_str() {
                    "Landed" => Ok(BundleStatus::Landed),
                    "Pending" => Ok(BundleStatus::Pending),
                    "Failed" => Ok(BundleStatus::Failed("Bundle failed".to_string())),
                    _ => Ok(BundleStatus::Pending),
                };
            }
        }
        
        Ok(BundleStatus::Pending)
    }
    
    /// Wait for bundle to land with timeout
    pub async fn wait_for_bundle(
        &self,
        bundle_id: &str,
        timeout_secs: u64,
    ) -> Result<BundleStatus> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        
        loop {
            if start.elapsed() > timeout {
                return Ok(BundleStatus::Expired);
            }
            
            let status = self.get_bundle_status(bundle_id).await?;
            
            match &status {
                BundleStatus::Landed => {
                    info!("Bundle {} landed successfully", bundle_id);
                    return Ok(status);
                }
                BundleStatus::Failed(reason) => {
                    warn!("Bundle {} failed: {}", bundle_id, reason);
                    return Ok(status);
                }
                BundleStatus::Pending => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                BundleStatus::Expired => {
                    return Ok(status);
                }
            }
        }
    }
    
    /// Create a tip instruction
    pub fn create_tip_instruction(
        &self,
        payer: &Pubkey,
    ) -> solana_sdk::instruction::Instruction {
        solana_sdk::system_instruction::transfer(
            payer,
            &self.get_tip_account(),
            self.tip_lamports,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tip_accounts_valid() {
        for account in JITO_TIP_ACCOUNTS {
            assert!(Pubkey::from_str(account).is_ok());
        }
    }
    
    #[test]
    fn test_bundle_status() {
        assert_eq!(BundleStatus::Pending, BundleStatus::Pending);
        assert_ne!(BundleStatus::Landed, BundleStatus::Pending);
    }
}
