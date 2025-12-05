# SOL Basis Trading Bot

An ultra-low-latency agentic basis trading bot for Solana that monitors funding rates for SOL perpetual futures, calculates basis spreads between spot and perp markets, and executes delta-neutral hedged positions with automated rebalancing.

## Features

- **Real-time Price Feeds**: Pyth oracle + Jupiter aggregator for spot, Drift Protocol for perps
- **Basis Spread Calculation**: Continuous monitoring of spot vs perp price differential
- **Funding Rate Analysis**: 8-hour rolling windows with annualized APR calculation
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
| 3 | Calculation Engines (funding, basis, signals) | ⏳ Pending |
| 4 | Execution (transactions, Jito, protocols) | ⏳ Pending |
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

## Current Functionality (Phase 2)

The bot now:
- Connects to Solana RPC with automatic failover
- Fetches SOL/USD prices from Pyth Network oracle
- Fetches spot prices from Jupiter aggregator
- Fetches SOL-PERP mark/index prices and funding rates from Drift
- Calculates real-time basis spread (perp vs spot)
- Computes annualized funding APR
- Broadcasts events through internal event bus
- Logs status updates every 10 seconds

Example output:
```
INFO  Starting SOL Basis Trading Bot v0.1.0
INFO  RPC health check passed (latency: 245ms)
INFO  Price feeds started
INFO  Status | Spot: $148.52 | Perp: $148.89 | Basis: 0.2491% | Funding APR: 18.42%
```

## Configuration

Edit `config.yaml` to configure:

- RPC endpoints and WebSocket URLs
- Trading parameters (min basis spread, max leverage, position sizes)
- Risk limits (max drawdown, stop loss, hedge drift threshold)
- Execution settings (Jito, priority fees, retries)
- Telemetry (logging, metrics, alerts)
- Protocol addresses (Drift, Pyth, Jupiter)

## Architecture

```
src/
├── main.rs              # Entry point + event loop
├── config/              # Configuration parsing
│   └── mod.rs
├── state/               # Thread-safe shared state
│   └── mod.rs
├── telemetry/           # Observability
│   ├── mod.rs
│   ├── logging.rs
│   ├── metrics.rs
│   └── alerts.rs
├── utils/               # Common types + helpers
│   ├── mod.rs
│   ├── types.rs
│   └── helpers.rs
├── network/             # Network layer
│   ├── mod.rs
│   ├── rpc_client.rs    # Solana RPC with failover
│   ├── websocket.rs     # WebSocket management
│   └── event_bus.rs     # Internal pub/sub
├── feeds/               # Price feeds
│   ├── mod.rs
│   ├── pyth.rs          # Pyth oracle
│   ├── jupiter.rs       # Jupiter aggregator
│   └── drift.rs         # Drift Protocol
├── engines/             # Calculation engines (Phase 3)
├── execution/           # Transaction handling (Phase 4)
├── agent/               # Agentic logic (Phase 5)
├── position/            # Position tracking (Phase 5)
└── protocols/           # Protocol SDKs (Phase 4)
```

## Requirements

- Rust 1.75+
- Solana RPC access (dedicated/private recommended for low latency)
- Wallet with SOL for trading (Phase 4+)

## Metrics

When metrics are enabled, Prometheus metrics are exposed on the configured port (default: 9090):

- `sol_basis_bot_spot_price` - Current SOL spot price
- `sol_basis_bot_perp_mark_price` - Current perp mark price
- `sol_basis_bot_basis_spread` - Basis spread percentage
- `sol_basis_bot_funding_apr` - Annualized funding APR
- `sol_basis_bot_trades_total` - Total trades executed
- `sol_basis_bot_execution_latency_ms` - Trade execution latency

## License

MIT

## Disclaimer

This software is for educational purposes. Trading involves significant risk. Use at your own discretion.
