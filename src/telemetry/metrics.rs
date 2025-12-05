//! Prometheus metrics export

use anyhow::Result;
use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing::info;

pub fn init_metrics(port: u16) -> Result<()> {
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    
    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()?;
    
    register_metrics();
    info!("Prometheus metrics server started on {}", addr);
    Ok(())
}

fn register_metrics() {
    // Price metrics
    describe_gauge!("sol_basis_bot_spot_price", "Current SOL spot price in USD");
    describe_gauge!("sol_basis_bot_perp_mark_price", "Current SOL perp mark price");
    describe_gauge!("sol_basis_bot_perp_index_price", "Current SOL perp index price");
    
    // Basis metrics
    describe_gauge!("sol_basis_bot_basis_spread", "Current basis spread percentage");
    describe_gauge!("sol_basis_bot_funding_rate", "Current hourly funding rate");
    describe_gauge!("sol_basis_bot_funding_apr", "Annualized funding APR percentage");
    describe_gauge!("sol_basis_bot_hedge_drift", "Current hedge drift percentage");
    
    // Position metrics
    describe_gauge!("sol_basis_bot_spot_position_size", "Current spot position size");
    describe_gauge!("sol_basis_bot_perp_position_size", "Current perp position size");
    describe_gauge!("sol_basis_bot_total_exposure_usd", "Total exposure in USD");
    
    // P&L metrics
    describe_gauge!("sol_basis_bot_realized_pnl", "Total realized P&L in USD");
    describe_gauge!("sol_basis_bot_unrealized_pnl", "Current unrealized P&L in USD");
    
    // Trade metrics
    describe_counter!("sol_basis_bot_trades_total", "Total number of trades executed");
    describe_counter!("sol_basis_bot_trades_success", "Number of successful trades");
    describe_counter!("sol_basis_bot_trades_failed", "Number of failed trades");
    
    // Latency metrics
    describe_histogram!("sol_basis_bot_execution_latency_ms", "Trade execution latency");
    describe_histogram!("sol_basis_bot_rpc_latency_us", "RPC request latency");
    
    // System metrics
    describe_counter!("sol_basis_bot_errors_total", "Total number of errors");
    describe_gauge!("sol_basis_bot_agent_state", "Current agent state");
    describe_gauge!("sol_basis_bot_rpc_connected", "RPC connection status");
    describe_gauge!("sol_basis_bot_ws_connected", "WebSocket connection status");
}

pub fn record_spot_price(price: f64) {
    gauge!("sol_basis_bot_spot_price").set(price);
}

pub fn record_perp_mark_price(price: f64) {
    gauge!("sol_basis_bot_perp_mark_price").set(price);
}

pub fn record_basis_spread(spread: f64) {
    gauge!("sol_basis_bot_basis_spread").set(spread);
}

pub fn record_funding_apr(apr: f64) {
    gauge!("sol_basis_bot_funding_apr").set(apr);
}

pub fn record_trade_success() {
    counter!("sol_basis_bot_trades_total").increment(1);
    counter!("sol_basis_bot_trades_success").increment(1);
}

pub fn record_trade_failure() {
    counter!("sol_basis_bot_trades_total").increment(1);
    counter!("sol_basis_bot_trades_failed").increment(1);
}

pub fn record_execution_latency(latency_ms: f64) {
    histogram!("sol_basis_bot_execution_latency_ms").record(latency_ms);
}

pub fn record_rpc_latency(latency_us: f64) {
    histogram!("sol_basis_bot_rpc_latency_us").record(latency_us);
}

pub fn record_error() {
    counter!("sol_basis_bot_errors_total").increment(1);
}

pub fn record_agent_state(state: u8) {
    gauge!("sol_basis_bot_agent_state").set(state as f64);
}

pub fn record_connection_status(rpc: bool, ws: bool) {
    gauge!("sol_basis_bot_rpc_connected").set(if rpc { 1.0 } else { 0.0 });
    gauge!("sol_basis_bot_ws_connected").set(if ws { 1.0 } else { 0.0 });
}
