//! Network module - Phase 2
//!
//! Provides RPC client, WebSocket management, and event bus for price feeds.

pub mod rpc_client;
pub mod websocket;
pub mod event_bus;

pub use rpc_client::RpcManager;
pub use websocket::WebSocketManager;
pub use event_bus::{EventBus, Event};
