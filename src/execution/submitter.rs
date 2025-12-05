//! Transaction Submitter
//!
//! Handles transaction submission with:
//! - Retry logic with exponential backoff
//! - Jito bundle support
//! - Confirmation waiting
//! - Error handling and recovery

use anyhow::{Context, Result};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::Signature,
    transaction::Transaction,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::AppConfig;
use crate::network::RpcManager;

/// Submission result
#[derive(Debug, Clone)]
pub struct SubmissionResult {
    /// Transaction signature
    pub signature: Signature,
    /// Slot when confirmed
    pub slot: Option<u64>,
    /// Number of retries needed
    pub retries: u32,
    /// Time to confirmation in milliseconds
    pub confirmation_time_ms: u64,
}

/// Submission error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionError {
    /// Transaction simulation failed
    SimulationFailed(String),
    /// Transaction expired (blockhash too old)
    Expired,
    /// Network error
    NetworkError(String),
    /// Insufficient funds
    InsufficientFunds,
    /// Max retries exceeded
    MaxRetriesExceeded,
    /// Unknown error
    Unknown(String),
}

/// Transaction submitter
pub struct TransactionSubmitter {
    /// Configuration
    config: Arc<AppConfig>,
    /// RPC manager
    rpc: Arc<RpcManager>,
}

impl TransactionSubmitter {
    /// Create a new submitter
    pub fn new(config: Arc<AppConfig>, rpc: Arc<RpcManager>) -> Self {
        Self { config, rpc }
    }
    
    /// Submit transaction with retry logic
    pub async fn submit_with_retry(
        &self,
        transaction: &Transaction,
    ) -> Result<SubmissionResult> {
        let max_retries = self.config.execution.max_retries;
        let retry_delay = Duration::from_millis(self.config.execution.retry_delay_ms);
        
        let start = std::time::Instant::now();
        let mut last_error = None;
        
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let backoff = retry_delay * (1 << (attempt - 1).min(4));
                debug!("Retry {} after {:?}", attempt, backoff);
                sleep(backoff).await;
            }
            
            match self.submit_once(transaction).await {
                Ok(signature) => {
                    info!("Transaction submitted: {}", signature);
                    
                    // Wait for confirmation
                    match self.wait_for_confirmation(&signature).await {
                        Ok(slot) => {
                            let elapsed = start.elapsed().as_millis() as u64;
                            info!(
                                "Transaction confirmed in slot {} ({} ms)",
                                slot, elapsed
                            );
                            
                            return Ok(SubmissionResult {
                                signature,
                                slot: Some(slot),
                                retries: attempt,
                                confirmation_time_ms: elapsed,
                            });
                        }
                        Err(e) => {
                            warn!("Confirmation failed: {}", e);
                            last_error = Some(e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Submission attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    
                    // Check if error is retryable
                    if !self.is_retryable_error(&last_error) {
                        break;
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }
    
    /// Submit transaction once
    async fn submit_once(&self, transaction: &Transaction) -> Result<Signature> {
        // Simulate first if configured
        if self.config.execution.simulate_before_submit {
            let sim_result = self.rpc.simulate_transaction(transaction).await?;
            if let Some(err) = sim_result.err {
                anyhow::bail!("Simulation failed: {:?}", err);
            }
        }
        
        self.rpc.send_transaction(transaction).await
    }
    
    /// Wait for transaction confirmation
    async fn wait_for_confirmation(&self, signature: &Signature) -> Result<u64> {
        let timeout = Duration::from_secs(30);
        let poll_interval = Duration::from_millis(500);
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Confirmation timeout");
            }
            
            // Check transaction status
            match self.rpc.get_client().get_signature_status(signature).await {
                Ok(Some(result)) => {
                    match result {
                        Ok(_) => {
                            // Get slot from transaction
                            let slot = self.rpc.get_slot().await.unwrap_or(0);
                            return Ok(slot);
                        }
                        Err(e) => {
                            anyhow::bail!("Transaction failed: {:?}", e);
                        }
                    }
                }
                Ok(None) => {
                    // Not yet confirmed
                    sleep(poll_interval).await;
                }
                Err(e) => {
                    warn!("Error checking status: {}", e);
                    sleep(poll_interval).await;
                }
            }
        }
    }
    
    /// Check if error is retryable
    fn is_retryable_error(&self, error: &Option<anyhow::Error>) -> bool {
        if let Some(e) = error {
            let msg = e.to_string().to_lowercase();
            
            // Not retryable errors
            if msg.contains("insufficient funds") {
                return false;
            }
            if msg.contains("account not found") {
                return false;
            }
            if msg.contains("invalid signature") {
                return false;
            }
            
            // Retryable errors
            if msg.contains("blockhash not found") {
                return true;
            }
            if msg.contains("timeout") {
                return true;
            }
            if msg.contains("connection") {
                return true;
            }
        }
        
        true
    }
    
    /// Submit transaction and don't wait for confirmation
    pub async fn submit_fire_and_forget(&self, transaction: &Transaction) -> Result<Signature> {
        self.submit_once(transaction).await
    }
    
    /// Submit multiple transactions in sequence
    pub async fn submit_batch(
        &self,
        transactions: &[Transaction],
    ) -> Vec<Result<SubmissionResult>> {
        let mut results = Vec::with_capacity(transactions.len());
        
        for (i, tx) in transactions.iter().enumerate() {
            debug!("Submitting transaction {} of {}", i + 1, transactions.len());
            results.push(self.submit_with_retry(tx).await);
            
            // Small delay between transactions
            sleep(Duration::from_millis(100)).await;
        }
        
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submission_error() {
        assert_eq!(SubmissionError::Expired, SubmissionError::Expired);
        assert_ne!(SubmissionError::Expired, SubmissionError::InsufficientFunds);
    }
}
