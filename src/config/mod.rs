//! Configuration module
//!
//! Handles loading and validation of the application configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub rpc: RpcConfig,
    pub wallet: WalletConfig,
    pub trading: TradingConfig,
    pub risk: RiskConfig,
    pub rebalance: RebalanceConfig,
    pub execution: ExecutionConfig,
    pub telemetry: TelemetryConfig,
    pub protocols: ProtocolsConfig,
    #[serde(default)]
    pub paper_trading: bool,
    #[serde(default)]
    pub devnet: bool,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse config file")?;
        
        config.validate()?;
        info!("Configuration loaded from {:?}", path);
        Ok(config)
    }
    
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.trading.min_basis_spread_pct > 0.0,
            "min_basis_spread_pct must be positive"
        );
        anyhow::ensure!(
            self.trading.max_leverage > 0.0 && self.trading.max_leverage <= 10.0,
            "max_leverage must be between 0 and 10"
        );
        anyhow::ensure!(
            self.trading.slippage_tolerance_pct > 0.0 && self.trading.slippage_tolerance_pct <= 5.0,
            "slippage_tolerance_pct must be between 0 and 5"
        );
        anyhow::ensure!(
            self.risk.max_drawdown_pct > 0.0 && self.risk.max_drawdown_pct <= 100.0,
            "max_drawdown_pct must be between 0 and 100"
        );
        anyhow::ensure!(
            self.risk.stop_loss_pct > 0.0 && self.risk.stop_loss_pct <= 50.0,
            "stop_loss_pct must be between 0 and 50"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub primary_url: String,
    #[serde(default)]
    pub fallback_urls: Vec<String>,
    pub ws_url: String,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_ms: u64,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u32,
}

fn default_connection_timeout() -> u64 { 5000 }
fn default_request_timeout() -> u64 { 10000 }
fn default_max_retries() -> u32 { 3 }
fn default_requests_per_second() -> u32 { 50 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    pub keypair_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub min_basis_spread_pct: f64,
    pub min_funding_apr_pct: f64,
    pub max_leverage: f64,
    pub max_position_size_sol: f64,
    pub max_total_exposure_usd: f64,
    pub slippage_tolerance_pct: f64,
    #[serde(default = "default_basis_close_threshold")]
    pub basis_close_threshold_pct: f64,
    #[serde(default = "default_max_hold_time")]
    pub max_hold_time_hours: u64,
}

fn default_basis_close_threshold() -> f64 { 0.05 }
fn default_max_hold_time() -> u64 { 168 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_drawdown_pct: f64,
    pub stop_loss_pct: f64,
    pub hedge_drift_threshold_pct: f64,
    pub max_funding_reversal_loss: f64,
    #[serde(default = "default_max_open_positions")]
    pub max_open_positions: u32,
    #[serde(default = "default_min_trade_interval")]
    pub min_trade_interval_secs: u64,
}

fn default_max_open_positions() -> u32 { 5 }
fn default_min_trade_interval() -> u64 { 60 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceConfig {
    pub check_interval_secs: u64,
    pub min_rebalance_size_sol: f64,
    #[serde(default = "default_max_rebalances")]
    pub max_rebalances_per_hour: u32,
}

fn default_max_rebalances() -> u32 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    pub use_jito: bool,
    pub jito_tip_lamports: u64,
    #[serde(default = "default_jito_url")]
    pub jito_block_engine_url: String,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub simulate_before_submit: bool,
    pub priority_fee: PriorityFeeConfig,
}

fn default_jito_url() -> String {
    "https://mainnet.block-engine.jito.wtf".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityFeeConfig {
    pub strategy: String,
    #[serde(default)]
    pub fixed_fee: u64,
    #[serde(default = "default_max_priority_fee")]
    pub max_fee: u64,
}

fn default_max_priority_fee() -> u64 { 100000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub log_level: String,
    #[serde(default)]
    pub json_logs: bool,
    pub log_file: Option<String>,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_true")]
    pub enable_metrics: bool,
    #[serde(default)]
    pub enable_alerts: bool,
    pub alert_webhook: Option<String>,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

fn default_metrics_port() -> u16 { 9090 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolsConfig {
    pub drift: DriftConfig,
    pub pyth: PythConfig,
    pub jupiter: JupiterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftConfig {
    pub program_id: String,
    pub market_index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythConfig {
    pub sol_usd_feed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterConfig {
    pub api_url: String,
    pub sol_mint: String,
    pub usdc_mint: String,
}
