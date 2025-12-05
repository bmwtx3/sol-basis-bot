//! Alert management for notifications

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

use crate::config::TelemetryConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlertLevel {
    pub fn emoji(&self) -> &str {
        match self {
            AlertLevel::Info => "ℹ️",
            AlertLevel::Warning => "⚠️",
            AlertLevel::Error => "❌",
            AlertLevel::Critical => "🚨",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub level: AlertLevel,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl Alert {
    pub fn new(level: AlertLevel, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            message: message.into(),
            timestamp: chrono::Utc::now().timestamp(),
            details: None,
        }
    }
    
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
    
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Info, title, message)
    }
    
    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Warning, title, message)
    }
    
    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Error, title, message)
    }
    
    pub fn critical(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertLevel::Critical, title, message)
    }
}

pub struct AlertManager {
    enabled: bool,
    webhook_url: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
    http_client: reqwest::Client,
}

impl AlertManager {
    pub fn new(config: &TelemetryConfig) -> Self {
        Self {
            enabled: config.enable_alerts,
            webhook_url: config.alert_webhook.clone(),
            telegram_bot_token: config.telegram.bot_token.clone(),
            telegram_chat_id: config.telegram.chat_id.clone(),
            http_client: reqwest::Client::new(),
        }
    }
    
    pub async fn send(&self, alert: Alert) {
        if !self.enabled {
            return;
        }
        
        match alert.level {
            AlertLevel::Info => info!("[ALERT] {}: {}", alert.title, alert.message),
            AlertLevel::Warning => warn!("[ALERT] {}: {}", alert.title, alert.message),
            AlertLevel::Error => error!("[ALERT] {}: {}", alert.title, alert.message),
            AlertLevel::Critical => error!("[CRITICAL] {}: {}", alert.title, alert.message),
        }
        
        if let Some(url) = &self.webhook_url {
            if let Err(e) = self.send_webhook(url, &alert).await {
                warn!("Failed to send webhook alert: {}", e);
            }
        }
        
        if self.telegram_bot_token.is_some() && self.telegram_chat_id.is_some() {
            if let Err(e) = self.send_telegram(&alert).await {
                warn!("Failed to send Telegram alert: {}", e);
            }
        }
    }
    
    async fn send_webhook(&self, url: &str, alert: &Alert) -> Result<()> {
        let payload = serde_json::json!({
            "text": format!("{} *{}*\n{}", alert.level.emoji(), alert.title, alert.message),
        });
        
        self.http_client.post(url).json(&payload).send().await?;
        Ok(())
    }
    
    async fn send_telegram(&self, alert: &Alert) -> Result<()> {
        let bot_token = self.telegram_bot_token.as_ref().unwrap();
        let chat_id = self.telegram_chat_id.as_ref().unwrap();
        
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let text = format!("{} *{}*\n\n{}", alert.level.emoji(), alert.title, alert.message);
        
        let payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown"
        });
        
        self.http_client.post(&url).json(&payload).send().await?;
        Ok(())
    }
}
