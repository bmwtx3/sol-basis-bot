# SOL Basis Trading Bot

An ultra-low-latency agentic basis trading bot for Solana that monitors funding rates for SOL perpetual futures, calculates basis spreads between spot and perp markets, and executes delta-neutral hedged positions with automated rebalancing.

## Features

- **Real-time Price Feeds**: Pyth oracle + Jupiter aggregator for spot, Drift Protocol for perps
- **Basis Spread Calculation**: Continuous monitoring of spot vs perp price differential
- **Funding Rate Analysis**: 8-hour rolling windows with annualized APR calculation
- **Signal Generation**: Automated trade signals based on basis + funding conditions
- **Delta-Neutral Hedging**: Automatic position sizing for market-neutral exposure
- **MEV Protection**: Jito bundle integration for atomic execution
- **Low Latency**: Lock-free data structures, QUIC transport, optimized hot paths
- **Risk Management**: Stop-loss, max drawdown, position limits, circuit breakers
- **Observability**: Prometheus metrics, structured logging, alerting

## Project Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Foundation (config, state, logging, types) | ✅ Complete |
| 2 | Network Layer (RPC, WebSocket, price feeds) | ✅ Complete |
| 3 | Calculation Engines (funding, basis, signals) | ✅ Complete |
| 4 | Execution (transactions, Jito, protocols) | ✅ Complete |
| 5 | Agent (state machine, risk, rebalancing) | ⏳ Pending |
| 6 | Production (testing, paper trading, docs) | ⏳ Pending |

## Quick Start

```bash
# Clone the repository
git clone https://github.com/bmwtx3/sol-basis-bot.git
cd sol-basis-bot

# Build in release mode
cargo build --release

# Run with default config
cargo run --release -- --config config.yaml

# Run in paper trading mode
cargo run --release -- --config config.yaml --paper

# Run on devnet
cargo run --release -- --config config.yaml --devnet
```

## Current Functionality (Phase 4)

The bot now has full execution infrastructure:
- **Transaction Builder**: Constructs Drift perp orders and Jupiter swap transactions
- **Jupiter Client**: Fetches quotes and builds swap transactions with slippage protection
- **Jito Client**: Submits bundles for MEV protection with tip rotation
- **Transaction Simulator**: Pre-flight validation and compute unit estimation
- **Transaction Submitter**: Retry logic with exponential backoff and confirmation waiting

### Execution Flow

1. Signal Engine generates trade signal (OpenBasis, CloseBasis, Rebalance)
2. Transaction Builder constructs atomic bundle:
   - Priority fee instruction
   - Jupiter swap (SOL ↔ USDC)
   - Drift perp order (long/short)
   - Jito tip (if using bundles)
3. Simulator validates transaction before submission
4. Submitter handles retry logic and confirmation

Example execution output:
```
INFO  Signal generated: OpenBasis | Size: 85.20 SOL | Confidence: 80.0%
INFO  Jupiter quote: 85200000000 -> 12780000000, price_impact: 0.01%
INFO  Built basis trade: spot=85.20 SOL, perp=85200000 (Short), priority_fee=1000
INFO  Simulation successful: 285000 compute units
INFO  Jito bundle submitted: abc123...
INFO  Bundle abc123... landed successfully
INFO  Transaction confirmed in slot 245678901 (1250 ms)
```

## Calculation Engines

### Funding Engine
- Tracks rolling 8-hour window of funding rate snapshots
- Calculates annualized APR: `rate × 24 × 365 × 100`
- Computes funding velocity (rate of change per hour)
- Detects elevated funding and reversal signals
- Predicts next funding payment

### Basis Engine
- Real-time spread calculation: `(perp - spot) / spot × 100`
- Historical percentile ranking
- Z-score computation for mean reversion
- Optimal hedge ratio calculation
- Hedge drift detection

### Signal Engine
- Evaluates open/close/rebalance conditions every 5 seconds
- Combines funding and basis signals
- Risk-adjusted position sizing
- Confidence scoring (0-100%)
- Expected profit estimation

## Execution Layer

### Transaction Builder
- Constructs Drift Protocol perp orders
- Builds atomic basis trade bundles (spot + perp)
- Dynamic priority fee calculation
- Compute budget optimization

### Jupiter Client
- Quote fetching with route optimization
- SOL ↔ USDC swap execution
- Slippage management
- Price impact calculation

### Jito Client
- Bundle submission for MEV protection
- Tip account rotation
- Bundle status tracking
- Automatic retry on failure

### Transaction Submitter
- Exponential backoff retry logic
- Confirmation waiting with timeout
- Error classification (retryable vs fatal)
- Batch submission support

## Configuration

Edit `config.yaml` to configure:

- RPC endpoints and WebSocket URLs
- Trading parameters (min basis spread, max leverage, position sizes)
- Risk limits (max drawdown, stop loss, hedge drift threshold)
- Execution settings (Jito, priority fees, retries)
- Telemetry (logging, metrics, alerts)
- Protocol addresses (Drift, Pyth, Jupiter)

### Key Trading Parameters

```yaml
trading:
  min_basis_spread_pct: 0.10     # Minimum 0.1% basis to open
  min_funding_apr_pct: 15.0      # Minimum 15% annualized funding
  max_leverage: 3.0              # Maximum 3x leverage
  max_position_size_sol: 1000.0  # Max 1000 SOL per leg
  basis_close_threshold_pct: 0.05 # Close when basis < 0.05%

risk:
  max_drawdown_pct: 5.0          # Pause at 5% drawdown
  hedge_drift_threshold_pct: 2.0 # Rebalance at 2% drift

execution:
  use_jito: true                 # Enable Jito bundles
  jito_tip_lamports: 10000       # 0.00001 SOL tip
  max_retries: 3                 # Retry attempts
  simulate_before_submit: true   # Pre-flight simulation
```

## Architecture

```
src/
├── main.rs              # Entry point + event loop
├── config/              # Configuration parsing
├── state/               # Thread-safe shared state
├── telemetry/           # Observability
│   ├── logging.rs
│   ├── metrics.rs
│   └── alerts.rs
├── network/             # Network layer
│   ├── rpc_client.rs    # Solana RPC with failover
│   ├── websocket.rs     # WebSocket management
│   └── event_bus.rs     # Internal pub/sub
├── feeds/               # Price feeds
│   ├── pyth.rs          # Pyth oracle
│   ├── jupiter.rs       # Jupiter aggregator
│   └── drift.rs         # Drift Protocol
├── engines/             # Calculation engines
│   ├── funding_engine.rs # Funding rate analysis
│   ├── basis_engine.rs   # Basis spread + hedge
│   └── signal_engine.rs  # Trade signal generation
├── execution/           # Transaction handling
│   ├── tx_builder.rs    # Transaction construction
│   ├── jupiter.rs       # Jupiter swap client
│   ├── jito.rs          # Jito bundle client
│   ├── simulator.rs     # Pre-flight simulation
│   └── submitter.rs     # Retry + confirmation
├── agent/               # Agentic logic (Phase 5)
├── position/            # Position tracking (Phase 5)
└── protocols/           # Protocol SDKs
```

## Requirements

- Rust 1.75+
- Solana RPC access (dedicated/private recommended for low latency)
- Wallet with SOL for trading

## Metrics

When metrics are enabled, Prometheus metrics are exposed on the configured port (default: 9090):

- `sol_basis_bot_spot_price` - Current SOL spot price
- `sol_basis_bot_perp_mark_price` - Current perp mark price
- `sol_basis_bot_basis_spread` - Basis spread percentage
- `sol_basis_bot_funding_apr` - Annualized funding APR
- `sol_basis_bot_trades_total` - Total trades executed
- `sol_basis_bot_execution_latency_ms` - Trade execution latency
- `sol_basis_bot_jito_bundles_submitted` - Jito bundles submitted
- `sol_basis_bot_jito_bundles_landed` - Jito bundles landed

## License

MIT

## Disclaimer

This software is for educational purposes. Trading involves significant risk. Use at your own discretion.
