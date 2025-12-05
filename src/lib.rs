//! SOL Basis Trading Bot Library
//!
//! This library provides all components for basis trading on Solana.

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

// Re-export main types
pub use config::AppConfig;
pub use state::SharedState;
pub use agent::{TradingAgent, AgentState};
pub use position::PositionManager;
pub use engines::EngineManager;
pub use network::{RpcManager, EventBus, Event};
