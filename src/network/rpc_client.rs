//! Solana RPC Client Manager
//!
//! Provides high-throughput RPC access with connection pooling,
//! automatic failover, and latency tracking.

use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::Hash,
    signature::Signature,
    transaction::Transaction,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::RpcConfig;

/// RPC Manager with failover support
pub struct RpcManager {
    /// Primary RPC client
    primary: Arc<RpcClient>,
    /// Fallback RPC clients
    fallbacks: Vec<Arc<RpcClient>>,
    /// Current active client index (0 = primary)
    active_index: RwLock<usize>,
    /// Configuration
    config: RpcConfig,
    /// Cached recent blockhash
    cached_blockhash: RwLock<Option<(Hash, Instant)>>,
    /// Blockhash cache duration
    blockhash_cache_duration: Duration,
}

impl RpcManager {
    /// Create a new RPC manager
    pub fn new(config: &RpcConfig) -> Result<Self> {
        let timeout = Duration::from_millis(config.request_timeout_ms);
        let commitment = CommitmentConfig::confirmed();
        
        let primary = Arc::new(RpcClient::new_with_timeout_and_commitment(
            config.primary_url.clone(),
            timeout,
            commitment,
        ));
        
        let fallbacks: Vec<Arc<RpcClient>> = config
            .fallback_urls
            .iter()
            .map(|url| {
                Arc::new(RpcClient::new_with_timeout_and_commitment(
                    url.clone(),
                    timeout,
                    commitment,
                ))
            })
            .collect();
        
        info!(
            "RPC Manager initialized with {} fallback endpoints",
            fallbacks.len()
        );
        
        Ok(Self {
            primary,
            fallbacks,
            active_index: RwLock::new(0),
            config: config.clone(),
            cached_blockhash: RwLock::new(None),
            blockhash_cache_duration: Duration::from_millis(400),
        })
    }
    
    /// Get the currently active RPC client
    pub async fn get_client(&self) -> Arc<RpcClient> {
        let index = *self.active_index.read().await;
        if index == 0 {
            self.primary.clone()
        } else {
            self.fallbacks.get(index - 1).cloned().unwrap_or(self.primary.clone())
        }
    }
    
    /// Switch to next available RPC endpoint
    pub async fn failover(&self) -> bool {
        let mut index = self.active_index.write().await;
        let total_endpoints = 1 + self.fallbacks.len();
        
        let next_index = (*index + 1) % total_endpoints;
        if next_index == *index {
            return false;
        }
        
        *index = next_index;
        warn!("RPC failover to endpoint index {}", next_index);
        true
    }
    
    /// Reset to primary endpoint
    pub async fn reset_to_primary(&self) {
        let mut index = self.active_index.write().await;
        *index = 0;
        info!("RPC reset to primary endpoint");
    }
    
    /// Get recent blockhash with caching
    pub async fn get_recent_blockhash(&self) -> Result<Hash> {
        // Check cache first
        {
            let cache = self.cached_blockhash.read().await;
            if let Some((hash, timestamp)) = &*cache {
                if timestamp.elapsed() < self.blockhash_cache_duration {
                    return Ok(*hash);
                }
            }
        }
        
        // Fetch new blockhash
        let client = self.get_client().await;
        let start = Instant::now();
        
        let blockhash = client
            .get_latest_blockhash()
            .await
            .context("Failed to get recent blockhash")?;
        
        debug!("Blockhash fetch took {:?}", start.elapsed());
        
        // Update cache
        {
            let mut cache = self.cached_blockhash.write().await;
            *cache = Some((blockhash, Instant::now()));
        }
        
        Ok(blockhash)
    }
    
    /// Get account balance
    pub async fn get_balance(&self, pubkey: &solana_sdk::pubkey::Pubkey) -> Result<u64> {
        let client = self.get_client().await;
        client
            .get_balance(pubkey)
            .await
            .context("Failed to get balance")
    }
    
    /// Send transaction with retry logic
    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<Signature> {
        let mut last_error = None;
        
        for attempt in 0..self.config.max_retries {
            let client = self.get_client().await;
            let start = Instant::now();
            
            match client.send_and_confirm_transaction(transaction).await {
                Ok(signature) => {
                    debug!(
                        "Transaction sent in {:?} on attempt {}",
                        start.elapsed(),
                        attempt + 1
                    );
                    return Ok(signature);
                }
                Err(e) => {
                    warn!("Transaction attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    
                    // Try failover on repeated failures
                    if attempt > 0 {
                        self.failover().await;
                    }
                    
                    tokio::time::sleep(Duration::from_millis(
                        100 * (attempt as u64 + 1),
                    ))
                    .await;
                }
            }
        }
        
        Err(last_error
            .map(|e| anyhow::anyhow!("{}", e))
            .unwrap_or_else(|| anyhow::anyhow!("Transaction failed after retries")))
    }
    
    /// Simulate transaction
    pub async fn simulate_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<solana_client::rpc_response::RpcSimulateTransactionResult> {
        let client = self.get_client().await;
        let result = client
            .simulate_transaction(transaction)
            .await
            .context("Failed to simulate transaction")?;
        Ok(result.value)
    }
    
    /// Get slot
    pub async fn get_slot(&self) -> Result<u64> {
        let client = self.get_client().await;
        client.get_slot().await.context("Failed to get slot")
    }
    
    /// Health check - test connection to current endpoint
    pub async fn health_check(&self) -> Result<Duration> {
        let client = self.get_client().await;
        let start = Instant::now();
        
        client
            .get_slot()
            .await
            .context("Health check failed")?;
        
        Ok(start.elapsed())
    }
    
    /// Get multiple accounts
    pub async fn get_multiple_accounts(
        &self,
        pubkeys: &[solana_sdk::pubkey::Pubkey],
    ) -> Result<Vec<Option<solana_sdk::account::Account>>> {
        let client = self.get_client().await;
        client
            .get_multiple_accounts(pubkeys)
            .await
            .context("Failed to get multiple accounts")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_manager_creation() {
        let config = RpcConfig {
            primary_url: "https://api.mainnet-beta.solana.com".to_string(),
            fallback_urls: vec!["https://solana-api.projectserum.com".to_string()],
            ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
            connection_timeout_ms: 5000,
            request_timeout_ms: 10000,
            max_retries: 3,
            requests_per_second: 50,
        };
        
        let manager = RpcManager::new(&config);
        assert!(manager.is_ok());
    }
}
