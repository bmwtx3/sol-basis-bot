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
| 2 | Network Layer (RPC, WebSocket, price feeds) | 🔄 In Progress |
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
├── main.rs              # Entry point
├── config/              # Configuration parsing
├── state/               # Shared state store (lock-free)
├── telemetry/           # Logging, metrics, alerts
├── utils/               # Types and helpers
├── network/             # RPC + WebSocket clients
├── feeds/               # Price feed processors
├── engines/             # Calculation engines
├── execution/           # Transaction handling
├── agent/               # State machine + risk
├── position/            # Position tracking
└── protocols/           # Protocol integrations
```

## Requirements

- Rust 1.75+
- Solana RPC access (dedicated/private recommended)
- Wallet with SOL for trading

## License

MIT

## Disclaimer

This software is for educational purposes. Trading involves significant risk. Use at your own discretion.
