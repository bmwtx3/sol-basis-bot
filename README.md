# SOL Basis Trading Bot

An ultra-low-latency agentic basis trading bot for Solana that monitors funding rates for SOL perpetual futures, calculates basis spreads between spot and perp markets, and executes delta-neutral hedged positions with automated rebalancing.

## Features

- **Real-time Price Feeds**: Pyth oracle + Jupiter aggregator for spot, Drift Protocol for perps
- **Basis Spread Calculation**: Continuous monitoring of spot vs perp price differential
- **Funding Rate Analysis**: 8-hour rolling windows with annualized APR calculation
- **Signal Generation**: Automated trade signals based on basis + funding conditions
- **Delta-Neutral Hedging**: Automatic position sizing for market-neutral exposure
- **MEV Protection**: Jito bundle integration for atomic execution
- **Low Latency**: Lock-free data structures, optimized hot paths
- **Risk Management**: Stop-loss, max drawdown, position limits, circuit breakers
- **Agentic Execution**: State machine for trade lifecycle management
- **Paper Trading**: Full simulation mode for testing
- **Observability**: Prometheus metrics, structured logging, alerting

## Project Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Foundation (config, state, logging, types) | ✅ Complete |
| 2 | Network Layer (RPC, WebSocket, price feeds) | ✅ Complete |
| 3 | Calculation Engines (funding, basis, signals) | ✅ Complete |
| 4 | Execution (transactions, Jito, protocols) | ✅ Complete |
| 5 | Agent (state machine, risk, rebalancing) | ✅ Complete |
| 6 | Production (testing, optimization, docs) | ✅ Complete |

**🎉 PROJECT COMPLETE - Ready for production use!**

## Quick Start

```bash
# Clone the repository
git clone https://github.com/bmwtx3/sol-basis-bot.git
cd sol-basis-bot

# Build in release mode
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Run in paper trading mode (recommended for testing)
cargo run --release -- --config config.yaml --paper

# Run with real execution
cargo run --release -- --config config.yaml

# Run on devnet
cargo run --release -- --config config.yaml --devnet
```

## Docker

```bash
# Build image
docker build -t sol-basis-bot .

# Run container
docker run -d --name sol-basis-bot \
  -v /path/to/keypair.json:/keys/keypair.json:ro \
  -v /path/to/config.yaml:/app/config.yaml:ro \
  -e SOLANA_KEYPAIR_PATH=/keys/keypair.json \
  -p 9090:9090 \
  sol-basis-bot
```

## Documentation

- [Trading Strategy Guide](docs/STRATEGY.md) - Understanding basis trading
- [Architecture Overview](docs/ARCHITECTURE.md) - Technical design
- [Deployment Guide](docs/DEPLOYMENT.md) - Production setup

## Architecture

```
src/
├── main.rs              # Entry point + event loop
├── lib.rs               # Library exports
├── config/              # Configuration parsing
├── state/               # Thread-safe shared state
├── telemetry/           # Observability
├── network/             # Network layer (RPC, WebSocket, events)
├── feeds/               # Price feeds (Pyth, Jupiter, Drift)
├── engines/             # Calculation engines
├── execution/           # Transaction handling
├── agent/               # Agentic logic
└── position/            # Position tracking
```

## Configuration

Edit `config.yaml`:

```yaml
paper_trading: true  # Start with paper trading!

trading:
  min_basis_spread_pct: 0.10
  min_funding_apr_pct: 15.0
  max_position_size_sol: 1000.0

risk:
  max_drawdown_pct: 5.0
  stop_loss_pct: 2.0

execution:
  use_jito: true
  simulate_before_submit: true
```

See [config.yaml](config.yaml) for full configuration options.

## Agent States

```
Idle → Opening → Monitoring → Closing → Idle
                     ↓
                Rebalancing
                     ↓
       Paused ←←←←←←←
```

## Risk Controls

| Control | Trigger | Action |
|---------|---------|--------|
| Max Drawdown | Equity drops 5% from peak | Pause + Close |
| Stop Loss | Position loss > 2% | Close |
| Hedge Drift | Spot/perp ratio > 2% | Rebalance |
| Daily Loss | P&L < -$500 | Pause |
| Error Rate | > 10 errors/hour | Pause |
| RPC Disconnect | Connection lost | Pause |

## Monitoring

Prometheus metrics on port 9090:
- `sol_basis_bot_spot_price`
- `sol_basis_bot_perp_mark_price`
- `sol_basis_bot_basis_spread`
- `sol_basis_bot_funding_apr`
- `sol_basis_bot_unrealized_pnl`
- `sol_basis_bot_realized_pnl`
- `sol_basis_bot_trades_total`
- `sol_basis_bot_agent_state`

Import [Grafana dashboard](monitoring/grafana-dashboard.json) for visualization.

## Example Output

```
INFO  ===========================================
INFO    SOL Basis Trading Bot - FULLY OPERATIONAL
INFO  ===========================================
INFO  Status | Spot: $148.52 | Perp: $148.89 | Basis: 0.25% | Funding APR: 18.42%
INFO  Trade signal: OpenBasis | Size: 85.20 SOL | Confidence: 80.0%
INFO  State transition: Idle -> Opening
INFO  Position opened: 85.20 SOL @ $148.52 (Long spot, Short perp)
INFO  State transition: Opening -> Monitoring
INFO  Status | Pos: 85.20 SOL | uPnL: $42.15
INFO  Basis converged to 0.05%, closing position
INFO  Position closed with P&L: $156.42
INFO  ===========================================
INFO    Session Summary
INFO    Trades: 2 | Realized P&L: $156.42
INFO  ===========================================
```

## Requirements

- Rust 1.75+
- Solana RPC access (dedicated/private recommended)
- Wallet with SOL for trading (paper mode doesn't require funds)

## Testing

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_tests

# Benchmarks
cargo bench
```

## License

MIT

## Disclaimer

This software is for educational purposes. Trading involves significant risk of loss. Use at your own discretion. Always test thoroughly in paper trading mode before using real funds.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test`
5. Submit a pull request

## Support

- GitHub Issues: https://github.com/bmwtx3/sol-basis-bot/issues
