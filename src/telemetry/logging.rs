//! Logging initialization

use anyhow::Result;
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::TelemetryConfig;

pub fn init_logging(config: &TelemetryConfig) -> Result<()> {
    let log_level = parse_log_level(&config.log_level);
    
    let env_filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy()
        .add_directive("hyper=warn".parse()?)
        .add_directive("reqwest=warn".parse()?)
        .add_directive("tungstenite=warn".parse()?)
        .add_directive("tokio_tungstenite=warn".parse()?);
    
    if config.json_logs {
        let fmt_layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);
        
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    } else {
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .compact();
        
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }
    
    Ok(())
}

fn parse_log_level(level: &str) -> Level {
    match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}
