//! Common types used throughout the application

use serde::{Deserialize, Serialize};
use std::fmt;

/// Agent state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Initializing,
    Scanning,
    Evaluating,
    Executing,
    Managing,
    Rebalancing,
    Unwinding,
    Paused,
    Error,
}

impl AgentState {
    pub fn code(&self) -> u8 {
        match self {
            AgentState::Initializing => 0,
            AgentState::Scanning => 1,
            AgentState::Evaluating => 2,
            AgentState::Executing => 3,
            AgentState::Managing => 4,
            AgentState::Rebalancing => 5,
            AgentState::Unwinding => 6,
            AgentState::Paused => 7,
            AgentState::Error => 8,
        }
    }
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentState::Initializing => write!(f, "INITIALIZING"),
            AgentState::Scanning => write!(f, "SCANNING"),
            AgentState::Evaluating => write!(f, "EVALUATING"),
            AgentState::Executing => write!(f, "EXECUTING"),
            AgentState::Managing => write!(f, "MANAGING"),
            AgentState::Rebalancing => write!(f, "REBALANCING"),
            AgentState::Unwinding => write!(f, "UNWINDING"),
            AgentState::Paused => write!(f, "PAUSED"),
            AgentState::Error => write!(f, "ERROR"),
        }
    }
}

/// Position side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
}

/// Position type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionType {
    Spot,
    Perpetual,
}

/// A trading position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub position_type: PositionType,
    pub side: PositionSide,
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub funding_payments: f64,
    pub opened_at: i64,
    pub updated_at: i64,
}

impl Position {
    pub fn new(
        id: String,
        position_type: PositionType,
        side: PositionSide,
        size: f64,
        entry_price: f64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            position_type,
            side,
            size,
            entry_price,
            mark_price: entry_price,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            funding_payments: 0.0,
            opened_at: now,
            updated_at: now,
        }
    }
    
    pub fn update_mark_price(&mut self, price: f64) {
        self.mark_price = price;
        self.unrealized_pnl = (price - self.entry_price) * self.size;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
    
    pub fn notional_value(&self) -> f64 {
        self.size.abs() * self.mark_price
    }
}

/// Funding rate snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingSnapshot {
    pub timestamp: i64,
    pub rate: f64,
    pub apr: f64,
}

/// Trade signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub signal_type: SignalType,
    pub size: f64,
    pub basis_spread: f64,
    pub funding_apr: f64,
    pub expected_profit: f64,
    pub confidence: f64,
    pub timestamp: i64,
    pub reason: String,
}

/// Signal types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    OpenBasis,
    CloseBasis,
    Rebalance,
    Hold,
}

/// Trade record for history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub id: String,
    pub trade_type: TradeType,
    pub position_type: PositionType,
    pub side: PositionSide,
    pub size: f64,
    pub price: f64,
    pub signature: String,
    pub fees: f64,
    pub slippage: f64,
    pub latency_ms: u64,
    pub timestamp: i64,
    pub success: bool,
    pub error: Option<String>,
}

/// Trade types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeType {
    Open,
    Close,
    Rebalance,
}

/// Price update from feeds
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    pub source: PriceSource,
    pub price: f64,
    pub confidence: Option<f64>,
    pub timestamp: i64,
}

/// Price sources
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    Pyth,
    Jupiter,
    DriftMark,
    DriftIndex,
}

impl fmt::Display for PriceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PriceSource::Pyth => write!(f, "Pyth"),
            PriceSource::Jupiter => write!(f, "Jupiter"),
            PriceSource::DriftMark => write!(f, "Drift Mark"),
            PriceSource::DriftIndex => write!(f, "Drift Index"),
        }
    }
}

pub type AppResult<T> = anyhow::Result<T>;
