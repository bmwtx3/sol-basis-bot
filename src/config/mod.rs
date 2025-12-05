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
    pub agentic: AgenticConfig,
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
        anyhow::ensure!(
            self.agentic.max_kelly_fraction > 0.0 && self.agentic.max_kelly_fraction <= 1.0,
            "max_kelly_fraction must be between 0 and 1"
        );
        Ok(())
    }
    
    /// Create a default config for testing
    #[cfg(test)]
    pub fn default_for_test() -> Self {
        Self {
            rpc: RpcConfig {
                primary_url: "https://api.mainnet-beta.solana.com".to_string(),
                fallback_urls: vec![],
                ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
                connection_timeout_ms: 5000,
                request_timeout_ms: 10000,
                max_retries: 3,
                requests_per_second: 50,
            },
            wallet: WalletConfig {
                keypair_path: "./wallet.json".to_string(),
            },
            trading: TradingConfig {
                min_basis_spread_pct: 0.1,
                min_funding_apr_pct: 15.0,
                max_leverage: 3.0,
                max_position_size_sol: 1000.0,
                max_total_exposure_usd: 100000.0,
                slippage_tolerance_pct: 0.5,
                basis_close_threshold_pct: 0.05,
                max_hold_time_hours: 168,
            },
            risk: RiskConfig {
                max_drawdown_pct: 5.0,
                stop_loss_pct: 2.0,
                hedge_drift_threshold_pct: 2.0,
                max_funding_reversal_loss: 500.0,
                max_open_positions: 5,
                min_trade_interval_secs: 60,
            },
            rebalance: RebalanceConfig {
                check_interval_secs: 60,
                min_rebalance_size_sol: 10.0,
                max_rebalances_per_hour: 10,
            },
            execution: ExecutionConfig {
                use_jito: true,
                jito_tip_lamports: 10000,
                jito_block_engine_url: "https://mainnet.block-engine.jito.wtf".to_string(),
                max_retries: 3,
                retry_delay_ms: 100,
                simulate_before_submit: true,
                priority_fee: PriorityFeeConfig {
                    strategy: "dynamic".to_string(),
                    fixed_fee: 1000,
                    max_fee: 100000,
                },
            },
            telemetry: TelemetryConfig {
                log_level: "info".to_string(),
                json_logs: false,
                log_file: None,
                metrics_port: 9090,
                enable_metrics: true,
                enable_alerts: false,
                alert_webhook: None,
                telegram: TelegramConfig::default(),
            },
            protocols: ProtocolsConfig {
                drift: DriftConfig {
                    program_id: "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH".to_string(),
                    market_index: 0,
                },
                pyth: PythConfig {
                    sol_usd_feed: "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG".to_string(),
                },
                jupiter: JupiterConfig {
                    api_url: "https://quote-api.jup.ag/v6".to_string(),
                    sol_mint: "So11111111111111111111111111111111111111112".to_string(),
                    usdc_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                },
            },
            agentic: AgenticConfig::default(),
            paper_trading: true,
            devnet: false,
        }
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

/// Agentic features configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticConfig {
    /// Enable adaptive position sizing based on performance
    #[serde(default = "default_true")]
    pub enable_adaptive_sizing: bool,
    
    /// Enable funding reversal detection
    #[serde(default = "default_true")]
    pub enable_reversal_detection: bool,
    
    /// Enable performance tracking/learning
    #[serde(default = "default_true")]
    pub enable_performance_tracking: bool,
    
    /// Path to performance database
    #[serde(default = "default_performance_db_path")]
    pub performance_db_path: String,
    
    /// Minimum trades before adaptive sizing kicks in
    #[serde(default = "default_min_trades_for_adaptation")]
    pub min_trades_for_adaptation: u32,
    
    /// Maximum Kelly fraction (safety cap)
    #[serde(default = "default_max_kelly_fraction")]
    pub max_kelly_fraction: f64,
    
    /// Use half-Kelly for safety
    #[serde(default = "default_true")]
    pub use_half_kelly: bool,
    
    /// Minimum position size multiplier (during drawdown)
    #[serde(default = "default_min_position_multiplier")]
    pub min_position_multiplier: f64,
    
    /// Alert cooldown for reversal detection (seconds)
    #[serde(default = "default_reversal_alert_cooldown")]
    pub reversal_alert_cooldown_secs: u64,
    
    /// Force close on critical reversal
    #[serde(default = "default_true")]
    pub force_close_on_critical_reversal: bool,
    
    /// Export trades to CSV periodically
    #[serde(default)]
    pub auto_export_trades: bool,
    
    /// CSV export path
    #[serde(default = "default_csv_export_path")]
    pub csv_export_path: String,
}

fn default_performance_db_path() -> String { "data/performance.json".to_string() }
fn default_min_trades_for_adaptation() -> u32 { 10 }
fn default_max_kelly_fraction() -> f64 { 0.25 }
fn default_min_position_multiplier() -> f64 { 0.2 }
fn default_reversal_alert_cooldown() -> u64 { 300 }
fn default_csv_export_path() -> String { "data/trades.csv".to_string() }

impl Default for AgenticConfig {
    fn default() -> Self {
        Self {
            enable_adaptive_sizing: true,
            enable_reversal_detection: true,
            enable_performance_tracking: true,
            performance_db_path: default_performance_db_path(),
            min_trades_for_adaptation: default_min_trades_for_adaptation(),
            max_kelly_fraction: default_max_kelly_fraction(),
            use_half_kelly: true,
            min_position_multiplier: default_min_position_multiplier(),
            reversal_alert_cooldown_secs: default_reversal_alert_cooldown(),
            force_close_on_critical_reversal: true,
            auto_export_trades: false,
            csv_export_path: default_csv_export_path(),
        }
    }
}
