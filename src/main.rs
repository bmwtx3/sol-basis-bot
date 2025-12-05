//! SOL Basis Trading Bot
//!
//! An ultra-low-latency agentic basis trading bot for Solana that:
//! - Monitors funding rates for SOL perpetual futures
//! - Calculates basis spread between spot and perp markets
//! - Executes delta-neutral hedged positions
//! - Automatically rebalances when conditions are met

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::{info, warn, error};

mod config;
mod state;
mod telemetry;
mod utils;
mod network;
mod feeds;
mod engines;
mod execution;
mod agent;
mod position;
mod protocols;

use config::AppConfig;
use state::SharedState;
use telemetry::{init_logging, init_metrics};

/// SOL Basis Trading Bot - Ultra-low-latency agentic trading
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    /// Enable paper trading mode (no real transactions)
    #[arg(long)]
    paper: bool,

    /// Enable devnet mode
    #[arg(long)]
    devnet: bool,

    /// Override log level
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Load configuration
    let mut config = AppConfig::load(&args.config)?;
    
    // Apply CLI overrides
    if args.paper {
        config.paper_trading = true;
    }
    if args.devnet {
        config.devnet = true;
    }
    if let Some(level) = args.log_level {
        config.telemetry.log_level = level;
    }

    // Initialize logging
    init_logging(&config.telemetry)?;
    
    info!("Starting SOL Basis Trading Bot v{}", env!("CARGO_PKG_VERSION"));
    info!("Paper trading: {}", config.paper_trading);
    info!("Devnet mode: {}", config.devnet);

    // Initialize metrics if enabled
    if config.telemetry.enable_metrics {
        init_metrics(config.telemetry.metrics_port)?;
        info!("Metrics server started on port {}", config.telemetry.metrics_port);
    }

    // Create shared state
    let state = Arc::new(SharedState::new());
    info!("Shared state initialized");

    // Wrap config in Arc for sharing
    let config = Arc::new(config);

    // TODO: Phase 2 - Initialize network layer
    // TODO: Phase 3 - Initialize calculation engines
    // TODO: Phase 4 - Initialize execution engine
    // TODO: Phase 5 - Initialize agent

    info!("Bot initialization complete - awaiting Phase 2 implementation");
    
    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received, gracefully stopping...");
        }
        Err(err) => {
            error!("Error listening for shutdown signal: {}", err);
        }
    }

    info!("SOL Basis Trading Bot stopped");
    Ok(())
}
