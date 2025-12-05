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
| 6 | Production (testing, optimization, docs) | ⏳ Pending |

## Quick Start

```bash
# Clone the repository
git clone https://github.com/bmwtx3/sol-basis-bot.git
cd sol-basis-bot

# Build in release mode
cargo build --release

# Run in paper trading mode (recommended for testing)
cargo run --release -- --config config.yaml --paper

# Run with real execution
cargo run --release -- --config config.yaml

# Run on devnet
cargo run --release -- --config config.yaml --devnet
```

## Current Functionality (Phase 5)

The bot is now fully operational with:

### Trading Agent
- **State Machine**: Idle → Opening → Monitoring → Closing/Rebalancing → Idle
- **Automatic Trade Execution**: Opens positions when conditions are met
- **Position Monitoring**: Tracks unrealized P&L in real-time
- **Automatic Closing**: Closes when basis converges or risk limits hit

### Risk Management
- **Max Drawdown**: Pauses trading when drawdown exceeds threshold
- **Stop Loss**: Closes positions at configurable loss percentage
- **Hedge Drift**: Monitors and alerts when hedge ratio drifts
- **Daily Loss Limits**: Tracks and enforces daily P&L limits
- **Error Rate**: Pauses on excessive errors
- **Connection Monitoring**: Pauses if RPC disconnects

### Position Management
- **Dual-Leg Tracking**: Spot (long) + Perp (short) positions
- **P&L Calculation**: Real-time unrealized and realized P&L
- **Funding Accumulation**: Tracks funding payments received
- **Trade History**: Maintains complete trade log

### Rebalancing
- **Hedge Drift Detection**: Monitors spot/perp ratio
- **Automatic Rebalancing**: Executes trades to restore 1:1 hedge
- **Rate Limiting**: Configurable max rebalances per hour
- **Minimum Size**: Ignores tiny drift amounts

Example output:
```
INFO  ===========================================
INFO    SOL Basis Trading Bot - FULLY OPERATIONAL
INFO  ===========================================
INFO  Status | Spot: $148.52 | Perp: $148.89 | Basis: 0.2491% | Funding APR: 18.42%
INFO  Trade signal: OpenBasis | Size: 85.20 SOL | Confidence: 80.0%
INFO  State transition: Idle -> Opening
INFO  Position opened: 85.20 SOL @ $148.52 (Long spot, Short perp)
INFO  State transition: Opening -> Monitoring
INFO  Status | Spot: $149.10 | Perp: $149.25 | Basis: 0.10% | Pos: 85.20 SOL | uPnL: $42.15
INFO  Basis converged to 0.05%, closing position
INFO  State transition: Monitoring -> Closing
INFO  Position closed: 85.20 SOL @ $149.10, P&L: $156.42
INFO  ===========================================
INFO    Session Summary
INFO    Trades: 2 | Realized P&L: $156.42
INFO  ===========================================
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
├── agent/               # Agentic logic
│   ├── state_machine.rs # Trade lifecycle states
│   ├── risk_manager.rs  # Risk controls + circuit breakers
│   └── rebalancer.rs    # Hedge rebalancing
└── position/            # Position tracking
    └── mod.rs           # P&L, trade history
```

## Configuration

Edit `config.yaml` to configure:

### Trading Parameters
```yaml
trading:
  min_basis_spread_pct: 0.10     # Minimum 0.1% basis to open
  min_funding_apr_pct: 15.0      # Minimum 15% annualized funding
  max_leverage: 3.0              # Maximum 3x leverage
  max_position_size_sol: 1000.0  # Max 1000 SOL per leg
  basis_close_threshold_pct: 0.05 # Close when basis < 0.05%
```

### Risk Parameters
```yaml
risk:
  max_drawdown_pct: 5.0          # Pause at 5% drawdown
  stop_loss_pct: 2.0             # Close at 2% loss
  hedge_drift_threshold_pct: 2.0 # Rebalance at 2% drift
  max_funding_reversal_loss: 500 # Max daily loss from funding
  min_trade_interval_secs: 60    # Minimum 60s between trades
```

### Rebalancing
```yaml
rebalance:
  check_interval_secs: 60        # Check every 60s
  min_rebalance_size_sol: 10.0   # Minimum 10 SOL to rebalance
  max_rebalances_per_hour: 10    # Rate limit
```

### Execution
```yaml
execution:
  use_jito: true                 # Enable Jito bundles
  jito_tip_lamports: 10000       # 0.00001 SOL tip
  max_retries: 3                 # Retry attempts
  simulate_before_submit: true   # Pre-flight simulation
```

## Agent States

| State | Description |
|-------|-------------|
| Idle | Waiting for trade opportunities |
| Opening | Executing entry trade (spot + perp) |
| Monitoring | Watching active position |
| Closing | Executing exit trade |
| Rebalancing | Adjusting hedge ratio |
| Paused | Risk-triggered halt |
| Error | Recovery state |

## Risk Controls

| Control | Trigger | Action |
|---------|---------|--------|
| Max Drawdown | Equity drops 5% from peak | Pause + Close |
| Stop Loss | Position loss > 2% | Close |
| Hedge Drift | Spot/perp ratio > 2% | Rebalance |
| Daily Loss | P&L < -$500 | Pause |
| Error Rate | > 10 errors/hour | Pause |
| RPC Disconnect | Connection lost | Pause |

## Requirements

- Rust 1.75+
- Solana RPC access (dedicated/private recommended)
- Wallet with SOL for trading (paper mode doesn't require funds)

## Metrics

Prometheus metrics on port 9090:
- `sol_basis_bot_spot_price` - Current SOL spot price
- `sol_basis_bot_perp_mark_price` - Current perp mark price
- `sol_basis_bot_basis_spread` - Basis spread percentage
- `sol_basis_bot_funding_apr` - Annualized funding APR
- `sol_basis_bot_unrealized_pnl` - Current unrealized P&L
- `sol_basis_bot_realized_pnl` - Total realized P&L
- `sol_basis_bot_trades_total` - Total trades executed
- `sol_basis_bot_agent_state` - Current agent state

## License

MIT

## Disclaimer

This software is for educational purposes. Trading involves significant risk of loss. Use at your own discretion. Always test thoroughly in paper trading mode before using real funds.
