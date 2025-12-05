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
use tracing::{info, warn, error, debug};

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
use network::{RpcManager, EventBus, Event};
use feeds::PriceFeedManager;
use engines::EngineManager;
use position::PositionManager;
use agent::TradingAgent;

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

    // Phase 2: Initialize network layer
    info!("Initializing network layer...");
    
    // Create event bus for internal communication
    let event_bus = EventBus::new(2048);
    let event_tx = event_bus.sender();
    info!("Event bus initialized");
    
    // Create RPC manager
    let rpc_manager = Arc::new(RpcManager::new(&config.rpc)?);
    info!("RPC manager initialized");
    
    // Test RPC connection
    match rpc_manager.health_check().await {
        Ok(latency) => {
            info!("RPC health check passed (latency: {:?})", latency);
            *state.rpc_connected.write() = true;
        }
        Err(e) => {
            warn!("RPC health check failed: {}", e);
        }
    }
    
    // Initialize price feeds
    info!("Initializing price feeds...");
    let price_feeds = PriceFeedManager::new(
        &config.protocols,
        state.clone(),
        event_tx.clone(),
    );
    
    // Start price feeds
    price_feeds.start().await?;
    info!("Price feeds started");
    
    // Phase 3: Initialize calculation engines
    info!("Initializing calculation engines...");
    let engine_manager = EngineManager::new(
        config.clone(),
        state.clone(),
        event_tx.clone(),
    );
    
    // Start engines
    engine_manager.start().await?;
    info!("Calculation engines started");
    
    // Phase 5: Initialize position manager and trading agent
    info!("Initializing position manager...");
    let position_manager = Arc::new(PositionManager::new(state.clone()));
    
    info!("Initializing trading agent...");
    let trading_agent = TradingAgent::new(
        config.clone(),
        state.clone(),
        position_manager.clone(),
        event_tx.clone(),
    );
    
    // Start trading agent
    trading_agent.start().await?;
    info!("Trading agent started");
    
    // Spawn event processor to update shared state
    let state_clone = state.clone();
    let position_manager_clone = position_manager.clone();
    let mut event_rx = event_bus.subscribe();
    let event_processor = tokio::spawn(async move {
        info!("Event processor started");
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    match event {
                        Event::SpotPriceUpdate(update) => {
                            state_clone.update_spot_price(update.price);
                            debug!("Spot price updated: ${:.4}", update.price);
                        }
                        Event::PerpMarkPriceUpdate(update) => {
                            state_clone.update_perp_mark_price(update.price);
                            debug!("Perp mark price updated: ${:.4}", update.price);
                            // Update position P&L
                            position_manager_clone.update_pnl().await;
                        }
                        Event::PerpIndexPriceUpdate(update) => {
                            state_clone.perp_index_price.store(update.price);
                            debug!("Perp index price updated: ${:.4}", update.price);
                        }
                        Event::FundingRateUpdate { rate, .. } => {
                            state_clone.update_funding_rate(rate);
                            debug!("Funding rate updated: {:.6}%", rate * 100.0);
                        }
                        Event::BasisSpreadUpdate { spread, spot_price, perp_price, .. } => {
                            debug!(
                                "Basis update: spread={:.4}%, spot=${:.2}, perp=${:.2}",
                                spread, spot_price, perp_price
                            );
                        }
                        Event::TradeSignal { signal_type, size, reason } => {
                            info!(
                                "Trade signal: {} | Size: {:.2} SOL | Reason: {}",
                                signal_type, size, reason
                            );
                        }
                        Event::PositionOpened { size, entry_price, side } => {
                            info!(
                                "Position opened: {:.4} SOL @ ${:.2} ({})",
                                size, entry_price, side
                            );
                        }
                        Event::PositionClosed { size, exit_price, pnl } => {
                            info!(
                                "Position closed: {:.4} SOL @ ${:.2}, P&L: ${:.2}",
                                size, exit_price, pnl
                            );
                        }
                        Event::SystemPause { reason } => {
                            warn!("System paused: {}", reason);
                        }
                        Event::SystemResume => {
                            info!("System resumed");
                        }
                        Event::WebSocketConnected => {
                            *state_clone.ws_connected.write() = true;
                            info!("WebSocket connected");
                        }
                        Event::WebSocketDisconnected => {
                            *state_clone.ws_connected.write() = false;
                            warn!("WebSocket disconnected");
                        }
                        Event::Error { source, message } => {
                            error!("Error from {}: {}", source, message);
                            state_clone.increment_error_count();
                        }
                        _ => {
                            debug!("Unhandled event received");
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event processor lagged by {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("Event bus closed");
                    break;
                }
            }
        }
    });
    
    // Spawn status reporter
    let state_clone = state.clone();
    let agent_for_status = trading_agent.current_state();
    let position_manager_for_status = position_manager.clone();
    let status_reporter = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            
            let spot = state_clone.spot_price.load();
            let perp = state_clone.perp_mark_price.load();
            let basis = state_clone.get_basis_spread();
            let funding_apr = state_clone.funding_apr.load();
            let positions = position_manager_for_status.get_positions().await;
            
            if spot > 0.0 && perp > 0.0 {
                if positions.spot_size > 0.0 {
                    info!(
                        "Status | Spot: ${:.2} | Perp: ${:.2} | Basis: {:.4}% | APR: {:.2}% | Pos: {:.2} SOL | uPnL: ${:.2}",
                        spot, perp, basis, funding_apr, positions.spot_size, positions.unrealized_pnl
                    );
                } else {
                    info!(
                        "Status | Spot: ${:.2} | Perp: ${:.2} | Basis: {:.4}% | Funding APR: {:.2}%",
                        spot, perp, basis, funding_apr
                    );
                }
            }
        }
    });

    info!("===========================================");
    info!("  SOL Basis Trading Bot - FULLY OPERATIONAL");
    info!("===========================================");
    info!("Monitoring prices, analyzing funding rates,");
    info!("generating signals, and executing trades...");
    
    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received, gracefully stopping...");
        }
        Err(err) => {
            error!("Error listening for shutdown signal: {}", err);
        }
    }
    
    // Cleanup
    info!("Stopping trading agent...");
    trading_agent.stop().await;
    
    info!("Stopping engines...");
    engine_manager.stop().await;
    
    info!("Stopping price feeds...");
    price_feeds.stop().await;
    
    event_processor.abort();
    status_reporter.abort();

    // Final P&L report
    let final_pnl = position_manager.get_realized_pnl().await;
    let trade_count = position_manager.get_trade_count().await;
    info!("===========================================");
    info!("  Session Summary");
    info!("  Trades: {} | Realized P&L: ${:.2}", trade_count, final_pnl);
    info!("===========================================");

    info!("SOL Basis Trading Bot stopped");
    Ok(())
}
