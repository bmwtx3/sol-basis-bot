//! Shared State Module
//!
//! Thread-safe state management using lock-free structures where possible.

use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::types::{AgentState, FundingSnapshot, Position};

/// Atomic floating point wrapper using u64 bit representation
#[derive(Debug, Default)]
pub struct AtomicF64 {
    inner: AtomicU64,
}

impl AtomicF64 {
    pub fn new(val: f64) -> Self {
        Self {
            inner: AtomicU64::new(val.to_bits()),
        }
    }

    pub fn load(&self) -> f64 {
        f64::from_bits(self.inner.load(Ordering::SeqCst))
    }

    pub fn store(&self, val: f64) {
        self.inner.store(val.to_bits(), Ordering::SeqCst);
    }
}

/// Central shared state store
pub struct SharedState {
    // Prices
    pub spot_price: AtomicF64,
    pub perp_mark_price: AtomicF64,
    pub perp_index_price: AtomicF64,
    pub last_price_update: AtomicI64,
    
    // Funding
    pub current_funding_rate: AtomicF64,
    pub funding_apr: AtomicF64,
    pub predicted_funding: AtomicF64,
    pub funding_history: DashMap<i64, FundingSnapshot>,
    
    // Basis
    pub basis_spread: AtomicF64,
    pub basis_history: DashMap<i64, f64>,
    pub hedge_drift: AtomicF64,
    
    // Positions
    pub spot_position: RwLock<Option<Position>>,
    pub perp_position: RwLock<Option<Position>>,
    pub open_positions: DashMap<String, Position>,
    
    // P&L
    pub realized_pnl: AtomicF64,
    pub unrealized_pnl: AtomicF64,
    pub total_funding_received: AtomicF64,
    
    // System
    pub agent_state: RwLock<AgentState>,
    pub last_rebalance: AtomicI64,
    pub last_trade: AtomicI64,
    pub error_count: AtomicU64,
    pub trade_count: AtomicU64,
    pub is_paused: RwLock<bool>,
    pub pause_reason: RwLock<Option<String>>,
    
    // Connection
    pub rpc_connected: RwLock<bool>,
    pub ws_connected: RwLock<bool>,
    pub rpc_latency_us: AtomicU64,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            spot_price: AtomicF64::new(0.0),
            perp_mark_price: AtomicF64::new(0.0),
            perp_index_price: AtomicF64::new(0.0),
            last_price_update: AtomicI64::new(0),
            current_funding_rate: AtomicF64::new(0.0),
            funding_apr: AtomicF64::new(0.0),
            predicted_funding: AtomicF64::new(0.0),
            funding_history: DashMap::new(),
            basis_spread: AtomicF64::new(0.0),
            basis_history: DashMap::new(),
            hedge_drift: AtomicF64::new(0.0),
            spot_position: RwLock::new(None),
            perp_position: RwLock::new(None),
            open_positions: DashMap::new(),
            realized_pnl: AtomicF64::new(0.0),
            unrealized_pnl: AtomicF64::new(0.0),
            total_funding_received: AtomicF64::new(0.0),
            agent_state: RwLock::new(AgentState::Initializing),
            last_rebalance: AtomicI64::new(0),
            last_trade: AtomicI64::new(0),
            error_count: AtomicU64::new(0),
            trade_count: AtomicU64::new(0),
            is_paused: RwLock::new(false),
            pause_reason: RwLock::new(None),
            rpc_connected: RwLock::new(false),
            ws_connected: RwLock::new(false),
            rpc_latency_us: AtomicU64::new(0),
        }
    }
    
    pub fn update_spot_price(&self, price: f64) {
        self.spot_price.store(price);
        self.update_price_timestamp();
        self.recalculate_basis();
    }
    
    pub fn update_perp_mark_price(&self, price: f64) {
        self.perp_mark_price.store(price);
        self.update_price_timestamp();
        self.recalculate_basis();
    }
    
    pub fn update_funding_rate(&self, rate: f64) {
        self.current_funding_rate.store(rate);
        let apr = rate * 24.0 * 365.0 * 100.0;
        self.funding_apr.store(apr);
        
        let timestamp = current_timestamp_millis();
        self.funding_history.insert(timestamp, FundingSnapshot {
            timestamp,
            rate,
            apr,
        });
        self.cleanup_funding_history();
    }
    
    pub fn get_basis_spread(&self) -> f64 {
        self.basis_spread.load()
    }
    
    fn recalculate_basis(&self) {
        let spot = self.spot_price.load();
        let perp = self.perp_mark_price.load();
        
        if spot > 0.0 {
            let basis = ((perp - spot) / spot) * 100.0;
            self.basis_spread.store(basis);
            
            let timestamp = current_timestamp_millis();
            self.basis_history.insert(timestamp, basis);
        }
    }
    
    fn update_price_timestamp(&self) {
        let now = current_timestamp_millis();
        self.last_price_update.store(now, Ordering::SeqCst);
    }
    
    fn cleanup_funding_history(&self) {
        let cutoff = current_timestamp_millis() - (8 * 60 * 60 * 1000);
        self.funding_history.retain(|&ts, _| ts > cutoff);
    }
    
    pub fn pause(&self, reason: &str) {
        *self.is_paused.write() = true;
        *self.pause_reason.write() = Some(reason.to_string());
        *self.agent_state.write() = AgentState::Paused;
    }
    
    pub fn resume(&self) {
        *self.is_paused.write() = false;
        *self.pause_reason.write() = None;
        *self.agent_state.write() = AgentState::Scanning;
    }
    
    pub fn increment_error_count(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }
    
    pub fn increment_trade_count(&self) {
        self.trade_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

fn current_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
