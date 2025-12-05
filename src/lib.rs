//! SOL Basis Trading Bot Library
//!
//! This library provides all components for basis trading on Solana.
//!
//! ## Agentic Features
//! 
//! The bot includes self-learning capabilities:
//! - **Performance Database**: Stores all trade outcomes, calculates win rate, Sharpe ratio, etc.
//! - **Adaptive Position Sizing**: Uses Kelly criterion adjusted by recent performance
//! - **Funding Reversal Detection**: Early warning system for funding rate reversals

pub mod config;
pub mod state;
pub mod telemetry;
pub mod utils;
pub mod network;
pub mod feeds;
pub mod engines;
pub mod execution;
pub mod agent;
pub mod position;
pub mod protocols;
pub mod agentic;

// Re-export main types
pub use config::AppConfig;
pub use state::SharedState;
pub use agent::{TradingAgent, AgentState};
pub use position::PositionManager;
pub use engines::EngineManager;
pub use network::{RpcManager, EventBus, Event};

// Re-export agentic types
pub use agentic::{
    PerformanceDb, TradeOutcome, PerformanceMetrics,
    AdaptiveSizer, SizingRecommendation,
    ReversalDetector, ReversalAlert, ReversalSeverity,
};
