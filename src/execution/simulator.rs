//! Transaction Simulator
//!
//! Pre-flight simulation of transactions:
//! - Compute unit estimation
//! - Error detection before submission
//! - Balance checks

use anyhow::{Context, Result};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    transaction::Transaction,
};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::network::RpcManager;

/// Simulation result
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// Whether simulation succeeded
    pub success: bool,
    /// Compute units consumed
    pub compute_units: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
    /// Logs from simulation
    pub logs: Vec<String>,
    /// Accounts that would be modified
    pub accounts_modified: Vec<String>,
}

/// Transaction simulator
pub struct TransactionSimulator {
    /// RPC manager
    rpc: Arc<RpcManager>,
}

impl TransactionSimulator {
    /// Create a new simulator
    pub fn new(rpc: Arc<RpcManager>) -> Self {
        Self { rpc }
    }
    
    /// Simulate a transaction
    pub async fn simulate(&self, transaction: &Transaction) -> Result<SimulationResult> {
        debug!("Simulating transaction...");
        
        let result = self.rpc.simulate_transaction(transaction).await?;
        
        let success = result.err.is_none();
        let error = result.err.map(|e| format!("{:?}", e));
        let logs = result.logs.unwrap_or_default();
        let compute_units = result.units_consumed;
        
        if success {
            info!(
                "Simulation successful: {} compute units",
                compute_units.unwrap_or(0)
            );
        } else {
            warn!("Simulation failed: {:?}", error);
            for log in &logs {
                debug!("  Log: {}", log);
            }
        }
        
        Ok(SimulationResult {
            success,
            compute_units,
            error,
            logs,
            accounts_modified: vec![],
        })
    }
    
    /// Simulate and check if transaction would succeed
    pub async fn would_succeed(&self, transaction: &Transaction) -> bool {
        match self.simulate(transaction).await {
            Ok(result) => result.success,
            Err(e) => {
                warn!("Simulation error: {}", e);
                false
            }
        }
    }
    
    /// Estimate compute units for a transaction
    pub async fn estimate_compute_units(&self, transaction: &Transaction) -> Result<u64> {
        let result = self.simulate(transaction).await?;
        
        result.compute_units
            .ok_or_else(|| anyhow::anyhow!("Could not estimate compute units"))
    }
    
    /// Check if we have sufficient balance for transaction fees
    pub async fn check_balance_for_tx(
        &self,
        payer: &solana_sdk::pubkey::Pubkey,
        estimated_fee: u64,
    ) -> Result<bool> {
        let balance = self.rpc.get_balance(payer).await?;
        
        // Need fee plus some buffer for rent
        let required = estimated_fee + 10_000; // 0.00001 SOL buffer
        
        if balance < required {
            warn!(
                "Insufficient balance: have {} lamports, need {} lamports",
                balance, required
            );
            return Ok(false);
        }
        
        debug!("Balance check passed: {} >= {}", balance, required);
        Ok(true)
    }
    
    /// Validate transaction before submission
    pub async fn validate_transaction(&self, transaction: &Transaction) -> Result<ValidationResult> {
        let mut issues = Vec::new();
        
        // Check signature count
        if transaction.signatures.is_empty() {
            issues.push("Transaction has no signatures".to_string());
        }
        
        // Check message
        let message = &transaction.message;
        
        if message.instructions.is_empty() {
            issues.push("Transaction has no instructions".to_string());
        }
        
        // Check for compute budget
        let has_compute_budget = message.instructions.iter().any(|ix| {
            let program_id = message.account_keys.get(ix.program_id_index as usize);
            program_id.map(|p| *p == solana_sdk::compute_budget::id()).unwrap_or(false)
        });
        
        if !has_compute_budget {
            issues.push("Transaction missing compute budget instruction".to_string());
        }
        
        // Simulate
        let sim_result = self.simulate(transaction).await?;
        
        if !sim_result.success {
            if let Some(error) = &sim_result.error {
                issues.push(format!("Simulation failed: {}", error));
            }
        }
        
        Ok(ValidationResult {
            valid: issues.is_empty() && sim_result.success,
            issues,
            simulation: Some(sim_result),
        })
    }
}

/// Transaction validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether transaction is valid
    pub valid: bool,
    /// List of issues found
    pub issues: Vec<String>,
    /// Simulation result
    pub simulation: Option<SimulationResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_result() {
        let result = SimulationResult {
            success: true,
            compute_units: Some(50000),
            error: None,
            logs: vec!["Program log: Success".to_string()],
            accounts_modified: vec![],
        };
        assert!(result.success);
        assert_eq!(result.compute_units, Some(50000));
    }
}
