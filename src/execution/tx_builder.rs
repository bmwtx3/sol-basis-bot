//! Transaction Builder
//!
//! Builds transactions for:
//! - Drift Protocol perp orders
//! - Jupiter swaps for spot
//! - Atomic basis trade bundles

use anyhow::{Context, Result};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info};

use crate::config::AppConfig;
use crate::network::RpcManager;

/// Drift order side
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Long,
    Short,
}

/// Drift order type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Market,
    Limit,
    TriggerMarket,
    TriggerLimit,
}

/// Drift order parameters
#[derive(Debug, Clone)]
pub struct DriftOrderParams {
    pub market_index: u16,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub base_asset_amount: u64,
    pub price: Option<u64>,
    pub reduce_only: bool,
}

/// Jupiter swap parameters
#[derive(Debug, Clone)]
pub struct SwapParams {
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount: u64,
    pub slippage_bps: u16,
}

/// Basis trade parameters (spot + perp)
#[derive(Debug, Clone)]
pub struct BasisTradeParams {
    /// SOL amount for spot leg
    pub spot_amount_sol: f64,
    /// Perp position size
    pub perp_size: u64,
    /// Perp side (opposite of spot)
    pub perp_side: OrderSide,
    /// Max slippage in basis points
    pub slippage_bps: u16,
}

/// Transaction builder
pub struct TransactionBuilder {
    /// Configuration
    config: Arc<AppConfig>,
    /// RPC manager
    rpc: Arc<RpcManager>,
    /// Drift program ID
    drift_program_id: Pubkey,
    /// Compute unit limit
    compute_units: u32,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new(config: Arc<AppConfig>, rpc: Arc<RpcManager>) -> Result<Self> {
        let drift_program_id = Pubkey::from_str(&config.protocols.drift.program_id)
            .context("Invalid Drift program ID")?;
        
        Ok(Self {
            config,
            rpc,
            drift_program_id,
            compute_units: 400_000, // Default compute units
        })
    }
    
    /// Build a priority fee instruction
    pub fn build_priority_fee_ix(&self, priority_fee: u64) -> Vec<Instruction> {
        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(self.compute_units),
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        ]
    }
    
    /// Build Drift place order instruction
    /// 
    /// Note: This is a simplified version. Full implementation would use
    /// the Drift SDK to construct proper account metas and instruction data.
    pub fn build_drift_place_order_ix(
        &self,
        user: &Pubkey,
        params: &DriftOrderParams,
    ) -> Result<Instruction> {
        // Drift place_perp_order instruction discriminator
        let discriminator: [u8; 8] = [69, 161, 93, 202, 120, 126, 76, 185];
        
        // Encode order params
        let mut data = discriminator.to_vec();
        
        // Order type (0 = Market, 1 = Limit, etc.)
        data.push(match params.order_type {
            OrderType::Market => 0,
            OrderType::Limit => 1,
            OrderType::TriggerMarket => 2,
            OrderType::TriggerLimit => 3,
        });
        
        // Market type (0 = Perp)
        data.push(0);
        
        // Direction (0 = Long, 1 = Short)
        data.push(match params.side {
            OrderSide::Long => 0,
            OrderSide::Short => 1,
        });
        
        // Market index
        data.extend_from_slice(&params.market_index.to_le_bytes());
        
        // Base asset amount
        data.extend_from_slice(&params.base_asset_amount.to_le_bytes());
        
        // Price (optional, 0 for market orders)
        let price = params.price.unwrap_or(0);
        data.extend_from_slice(&price.to_le_bytes());
        
        // Reduce only flag
        data.push(if params.reduce_only { 1 } else { 0 });
        
        // In production, we'd need proper account metas for:
        // - State account
        // - User account
        // - User stats account
        // - Perp market account
        // - Oracle account
        // - Authority
        
        debug!(
            "Built Drift order: market={}, side={:?}, size={}, price={:?}",
            params.market_index, params.side, params.base_asset_amount, params.price
        );
        
        Ok(Instruction {
            program_id: self.drift_program_id,
            accounts: vec![
                // Placeholder - would need actual accounts
            ],
            data,
        })
    }
    
    /// Build a complete basis trade transaction bundle
    pub async fn build_basis_trade(
        &self,
        payer: &Keypair,
        params: &BasisTradeParams,
        swap_instructions: Vec<Instruction>,
    ) -> Result<Transaction> {
        let mut instructions = Vec::new();
        
        // 1. Add priority fee
        let priority_fee = self.get_dynamic_priority_fee().await?;
        instructions.extend(self.build_priority_fee_ix(priority_fee));
        
        // 2. Add spot swap (Jupiter instructions)
        instructions.extend(swap_instructions);
        
        // 3. Add perp order
        let perp_order = DriftOrderParams {
            market_index: self.config.protocols.drift.market_index,
            side: params.perp_side,
            order_type: OrderType::Market,
            base_asset_amount: params.perp_size,
            price: None,
            reduce_only: false,
        };
        instructions.push(self.build_drift_place_order_ix(&payer.pubkey(), &perp_order)?);
        
        // Get recent blockhash
        let blockhash = self.rpc.get_recent_blockhash().await?;
        
        // Build transaction
        let message = Message::new(&instructions, Some(&payer.pubkey()));
        let mut tx = Transaction::new_unsigned(message);
        tx.partial_sign(&[payer], blockhash);
        
        info!(
            "Built basis trade: spot={:.4} SOL, perp={} ({:?}), priority_fee={}",
            params.spot_amount_sol, params.perp_size, params.perp_side, priority_fee
        );
        
        Ok(tx)
    }
    
    /// Build close position transaction
    pub async fn build_close_position(
        &self,
        payer: &Keypair,
        spot_amount: u64,
        perp_size: u64,
        current_perp_side: OrderSide,
    ) -> Result<Transaction> {
        let mut instructions = Vec::new();
        
        // Priority fee
        let priority_fee = self.get_dynamic_priority_fee().await?;
        instructions.extend(self.build_priority_fee_ix(priority_fee));
        
        // Close perp (opposite side)
        let close_side = match current_perp_side {
            OrderSide::Long => OrderSide::Short,
            OrderSide::Short => OrderSide::Long,
        };
        
        let close_perp = DriftOrderParams {
            market_index: self.config.protocols.drift.market_index,
            side: close_side,
            order_type: OrderType::Market,
            base_asset_amount: perp_size,
            price: None,
            reduce_only: true,
        };
        instructions.push(self.build_drift_place_order_ix(&payer.pubkey(), &close_perp)?);
        
        // Note: Would also need Jupiter swap to convert USDC back to SOL
        // or close spot position
        
        let blockhash = self.rpc.get_recent_blockhash().await?;
        let message = Message::new(&instructions, Some(&payer.pubkey()));
        let mut tx = Transaction::new_unsigned(message);
        tx.partial_sign(&[payer], blockhash);
        
        info!("Built close position: spot={}, perp={}", spot_amount, perp_size);
        
        Ok(tx)
    }
    
    /// Get dynamic priority fee based on network conditions
    async fn get_dynamic_priority_fee(&self) -> Result<u64> {
        match self.config.execution.priority_fee.strategy.as_str() {
            "fixed" => Ok(self.config.execution.priority_fee.fixed_fee),
            "dynamic" => {
                // In production, would query recent priority fees
                // For now, use a reasonable default
                let base_fee = 1000u64; // 1000 micro-lamports
                let max_fee = self.config.execution.priority_fee.max_fee;
                Ok(base_fee.min(max_fee))
            }
            _ => Ok(self.config.execution.priority_fee.fixed_fee),
        }
    }
    
    /// Set compute unit limit
    pub fn set_compute_units(&mut self, units: u32) {
        self.compute_units = units;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_side() {
        assert_ne!(OrderSide::Long, OrderSide::Short);
    }
    
    #[test]
    fn test_order_type() {
        assert_ne!(OrderType::Market, OrderType::Limit);
    }
}
