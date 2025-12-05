//! Helper functions

use std::time::{SystemTime, UNIX_EPOCH, Instant};
use solana_sdk::signature::Keypair;
use anyhow::{Result, Context};
use std::path::Path;

pub fn current_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub fn current_timestamp_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

pub fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

pub fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros() as u64
}

pub fn load_keypair(path: &Path) -> Result<Keypair> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read keypair file: {:?}", path))?;
    
    let bytes: Vec<u8> = serde_json::from_str(&content)
        .with_context(|| "Failed to parse keypair JSON")?;
    
    Keypair::from_bytes(&bytes)
        .map_err(|e| anyhow::anyhow!("Invalid keypair: {}", e))
}

pub fn load_keypair_from_env_or_file(env_var: &str, file_path: &Path) -> Result<Keypair> {
    if let Ok(key_str) = std::env::var(env_var) {
        if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&key_str) {
            return Keypair::from_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("Invalid keypair from env: {}", e));
        }
        if let Ok(bytes) = bs58::decode(&key_str).into_vec() {
            return Keypair::from_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("Invalid keypair from env: {}", e));
        }
    }
    load_keypair(file_path)
}

pub fn format_price(price: f64) -> String {
    if price >= 1000.0 {
        format!("{:.2}", price)
    } else if price >= 1.0 {
        format!("{:.4}", price)
    } else {
        format!("{:.6}", price)
    }
}

pub fn format_percentage(pct: f64) -> String {
    format!("{:.4}%", pct)
}

pub fn format_usd(amount: f64) -> String {
    if amount.abs() >= 1_000_000.0 {
        format!("${:.2}M", amount / 1_000_000.0)
    } else if amount.abs() >= 1_000.0 {
        format!("${:.2}K", amount / 1_000.0)
    } else {
        format!("${:.2}", amount)
    }
}

pub fn annualize_return(period_return: f64, period_hours: f64) -> f64 {
    let periods_per_year = 365.0 * 24.0 / period_hours;
    ((1.0 + period_return).powf(periods_per_year) - 1.0) * 100.0
}

pub fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 { 0.0 } else { numerator / denominator }
}

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub async fn retry_with_backoff<T, E, F, Fut>(
    mut operation: F,
    max_retries: u32,
    initial_delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = initial_delay_ms;
    let mut last_error = None;
    
    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    delay *= 2;
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}
