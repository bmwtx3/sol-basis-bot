# Architecture Overview

This document describes the technical architecture of the SOL Basis Trading Bot.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Trading Agent                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │   State     │  │    Risk     │  │  Position   │  │ Rebalancer │  │
│  │  Machine    │  │  Manager    │  │  Manager    │  │            │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └────────────┘  │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────────────┐
│                     Event Bus                                        │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  broadcast::channel<Event>  (Lock-free pub/sub)              │   │
│  └──────────────────────────────────────────────────────────────┘   │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌───────────────┐  ┌───────────────┐  ┌───────────────────────────────┐
│ Price Feeds   │  │   Engines     │  │       Execution Layer         │
├───────────────┤  ├───────────────┤  ├───────────────────────────────┤
│ • Pyth        │  │ • Funding     │  │ • Transaction Builder         │
│ • Jupiter     │  │ • Basis       │  │ • Jupiter Client              │
│ • Drift       │  │ • Signal      │  │ • Jito Client                 │
└───────┬───────┘  └───────┬───────┘  │ • Simulator                   │
        │                  │          │ • Submitter                   │
        ▼                  ▼          └───────────────┬───────────────┘
┌───────────────────────────────────┐                 │
│          Shared State             │                 │
│  ┌─────────────────────────────┐  │                 ▼
│  │ AtomicF64 prices, rates     │  │  ┌───────────────────────────┐
│  │ RwLock<Position> positions  │  │  │      Solana Network       │
│  │ AtomicU32 counters          │  │  │  • RPC (HTTP + WebSocket) │
│  └─────────────────────────────┘  │  │  • Pyth Oracle            │
└───────────────────────────────────┘  │  • Jupiter Aggregator     │
                                       │  • Drift Protocol         │
                                       │  • Jito Block Engine      │
                                       └───────────────────────────┘
```

## Component Details

### 1. Trading Agent (`src/agent/`)

The brain of the system. Coordinates all trading logic.

**State Machine** (`state_machine.rs`)
- Manages trade lifecycle states
- Enforces valid state transitions
- Tracks time in each state

```
States: Idle → Opening → Monitoring → Closing → Idle
                            ↓
                       Rebalancing
                            ↓
        Paused ←←←←←←←←←←←←←
```

**Risk Manager** (`risk_manager.rs`)
- Real-time risk monitoring
- Circuit breaker triggers
- Drawdown calculation
- Daily P&L tracking

**Rebalancer** (`rebalancer.rs`)
- Hedge drift detection
- Rebalance execution
- Rate limiting

### 2. Position Manager (`src/position/`)

Tracks all position data:
- Spot position (size, entry, value)
- Perp position (size, entry, funding)
- Realized and unrealized P&L
- Trade history

### 3. Calculation Engines (`src/engines/`)

**Funding Engine** (`funding_engine.rs`)
- 8-hour rolling window analysis
- Annualized APR calculation
- Funding velocity tracking
- Reversal detection

**Basis Engine** (`basis_engine.rs`)
- Real-time spread calculation
- Historical percentiles
- Z-score analysis
- Position sizing

**Signal Engine** (`signal_engine.rs`)
- Multi-factor evaluation
- Confidence scoring
- Trade signal generation

### 4. Price Feeds (`src/feeds/`)

**Pyth Feed** (`pyth.rs`)
- SOL/USD oracle price
- Confidence intervals
- Price staleness checks

**Jupiter Feed** (`jupiter.rs`)
- SOL/USDC aggregated price
- Best route pricing
- Slippage estimation

**Drift Feed** (`drift.rs`)
- SOL-PERP mark price
- Index price
- Funding rate

### 5. Execution Layer (`src/execution/`)

**Transaction Builder** (`tx_builder.rs`)
- Constructs Solana transactions
- Priority fee calculation
- Drift order encoding

**Jupiter Client** (`jupiter.rs`)
- Quote fetching
- Swap transaction building
- Slippage handling

**Jito Client** (`jito.rs`)
- Bundle submission
- Tip account rotation
- Status polling

**Simulator** (`simulator.rs`)
- Pre-flight simulation
- Compute unit estimation
- Balance checks

**Submitter** (`submitter.rs`)
- Retry logic with backoff
- Confirmation waiting
- Error classification

### 6. Network Layer (`src/network/`)

**RPC Manager** (`rpc_client.rs`)
- Connection pooling
- Automatic failover
- Blockhash caching
- Health monitoring

**WebSocket Manager** (`websocket.rs`)
- Account subscriptions
- Auto-reconnection
- Heartbeat monitoring

**Event Bus** (`event_bus.rs`)
- Lock-free pub/sub
- Type-safe events
- Bounded channels

### 7. Shared State (`src/state/`)

Thread-safe state using:
- `AtomicU64` for prices (via AtomicF64 wrapper)
- `AtomicU32` for counters
- `parking_lot::RwLock` for complex types

```rust
pub struct SharedState {
    // Prices (lock-free)
    pub spot_price: AtomicF64,
    pub perp_mark_price: AtomicF64,
    
    // Positions (RwLock for complex updates)
    pub spot_position: RwLock<Option<Position>>,
    pub perp_position: RwLock<Option<Position>>,
    
    // Counters (atomic)
    pub error_count: AtomicU32,
}
```

## Data Flow

### Price Update Flow

```
1. Pyth Oracle → WebSocket subscription
2. Account data change notification
3. Parse price from account
4. Store in AtomicF64 (lock-free)
5. Emit SpotPriceUpdate event
6. Basis engine calculates spread
7. Signal engine evaluates conditions
```

### Trade Execution Flow

```
1. Signal engine emits TradeSignal
2. Trading agent receives signal
3. State machine transitions to Opening
4. Position manager calculates size
5. TX builder creates atomic bundle:
   a. Priority fee instruction
   b. Jupiter swap (spot)
   c. Drift place order (perp)
6. Simulator validates transaction
7. Jito client submits bundle
8. Submitter waits for confirmation
9. Position manager records trade
10. State machine transitions to Monitoring
```

### Risk Check Flow

```
1. Risk manager runs every 1 second
2. Calculate current drawdown
3. Check unrealized P&L
4. Verify hedge drift
5. Monitor error count
6. Check connection status
7. If any limit exceeded:
   a. Set should_pause = true
   b. State machine → Paused
   c. Emit SystemPause event
```

## Performance Optimizations

### Lock-Free Price Updates

Using `AtomicU64` with bit casting:
```rust
impl AtomicF64 {
    fn store(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Release);
    }
    fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}
```

### Bounded Event Channels

```rust
let (tx, rx) = broadcast::channel::<Event>(2048);
// Subscribers can lag, events are dropped if buffer full
```

### Zero-Copy Deserialization

For Solana account data:
```rust
let price_data: &PriceAccount = bytemuck::from_bytes(&account.data);
```

### Connection Pooling

RPC client maintains connection pool:
```rust
RpcClient::new_with_timeout_and_commitment(
    url,
    Duration::from_millis(timeout),
    commitment,
)
```

## Error Handling

### Error Categories

| Category | Action |
|----------|--------|
| Transient (timeout, 429) | Retry with backoff |
| Permanent (invalid signature) | Fail immediately |
| Network (disconnect) | Reconnect, pause trading |
| Protocol (insufficient funds) | Alert, pause trading |

### Recovery Strategies

1. **Automatic retry**: For transient errors
2. **Reconnection**: For dropped connections
3. **Circuit breaker**: For cascading failures
4. **Manual intervention**: For protocol issues

## Testing Strategy

### Unit Tests

Each module has `#[cfg(test)]` tests:
- State machine transitions
- Calculation correctness
- Configuration validation

### Integration Tests

`tests/integration_tests.rs`:
- Full trade cycle simulation
- Adverse scenario handling
- Risk limit enforcement

### Benchmarks

`benches/performance.rs`:
- Atomic operations
- Statistical calculations
- Event processing throughput

## Security Considerations

### Keypair Handling

- Never logged or serialized
- Loaded from file at startup
- Stored in memory only

### Input Validation

- All config values validated
- RPC responses verified
- Price staleness checked

### Rate Limiting

- API calls throttled
- Rebalance frequency capped
- Error count monitored

## Extension Points

### Adding New Price Feeds

1. Implement in `src/feeds/`
2. Emit standard `PriceUpdate` events
3. Register in `PriceFeedManager`

### Adding New Protocols

1. Implement in `src/protocols/`
2. Add execution support
3. Update transaction builder

### Adding New Risk Checks

1. Add method to `RiskManager`
2. Include in `check_all()`
3. Define appropriate thresholds
