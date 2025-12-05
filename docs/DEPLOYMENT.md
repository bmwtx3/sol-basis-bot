# Deployment Guide

This guide covers deploying the SOL Basis Trading Bot in production.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Infrastructure Setup](#infrastructure-setup)
3. [Configuration](#configuration)
4. [Deployment Options](#deployment-options)
5. [Monitoring](#monitoring)
6. [Security](#security)
7. [Troubleshooting](#troubleshooting)

## Prerequisites

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 2 cores | 4+ cores |
| RAM | 4 GB | 8+ GB |
| Disk | 20 GB SSD | 50+ GB NVMe |
| Network | 100 Mbps | 1 Gbps |

### Software Requirements

- Rust 1.75+
- Linux (Ubuntu 22.04+ recommended)
- Solana CLI tools (optional, for key management)

### Network Requirements

- Low-latency connection to Solana RPC
- Recommended: Co-located with RPC provider
- Fallback RPC endpoints configured

## Infrastructure Setup

### 1. Build from Source

```bash
# Clone repository
git clone https://github.com/bmwtx3/sol-basis-bot.git
cd sol-basis-bot

# Build release binary
cargo build --release

# Binary location
./target/release/sol-basis-bot
```

### 2. Create Systemd Service

```bash
sudo nano /etc/systemd/system/sol-basis-bot.service
```

```ini
[Unit]
Description=SOL Basis Trading Bot
After=network.target

[Service]
Type=simple
User=solbot
Group=solbot
WorkingDirectory=/opt/sol-basis-bot
ExecStart=/opt/sol-basis-bot/sol-basis-bot --config /opt/sol-basis-bot/config.yaml
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/sol-basis-bot/logs

# Resource limits
MemoryLimit=2G
CPUQuota=200%

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable sol-basis-bot
sudo systemctl start sol-basis-bot
```

### 3. Log Rotation

```bash
sudo nano /etc/logrotate.d/sol-basis-bot
```

```
/opt/sol-basis-bot/logs/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    create 0640 solbot solbot
}
```

## Configuration

### Production Config Template

```yaml
# config.yaml - Production

paper_trading: false  # DANGER: Set to true for testing!
devnet: false

rpc:
  endpoints:
    - url: "https://your-primary-rpc.com"
      weight: 100
    - url: "https://your-backup-rpc.com"
      weight: 50
  request_timeout_ms: 3000
  max_retries: 3

protocols:
  pyth:
    sol_usd_feed: "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG"
    update_interval_ms: 500
  jupiter:
    api_url: "https://quote-api.jup.ag/v6"
    sol_mint: "So11111111111111111111111111111111111111112"
    usdc_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
  drift:
    program_id: "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH"
    sol_perp_market_index: 0
    update_interval_ms: 500

trading:
  min_basis_spread_pct: 0.15
  min_funding_apr_pct: 20.0
  max_leverage: 2.0
  max_position_size_sol: 500.0
  target_position_size_sol: 200.0
  basis_close_threshold_pct: 0.05
  min_trade_interval_secs: 120

risk:
  max_drawdown_pct: 3.0
  stop_loss_pct: 1.5
  hedge_drift_threshold_pct: 1.5
  max_funding_reversal_loss: 200.0

rebalance:
  check_interval_secs: 30
  min_rebalance_size_sol: 10.0
  max_rebalances_per_hour: 6

execution:
  use_jito: true
  jito_block_engine_url: "https://mainnet.block-engine.jito.wtf"
  jito_tip_lamports: 50000
  max_retries: 3
  retry_delay_ms: 200
  simulate_before_submit: true
  slippage_bps: 30
  priority_fee_percentile: 90

telemetry:
  log_level: "info"
  log_file: "/opt/sol-basis-bot/logs/bot.log"
  enable_metrics: true
  metrics_port: 9090
  alert_webhook_url: "https://hooks.slack.com/..."
```

### Environment Variables

```bash
# Required
export SOLANA_KEYPAIR_PATH="/path/to/keypair.json"

# Optional
export RUST_LOG="sol_basis_bot=info"
export SOL_BASIS_BOT_CONFIG="/opt/sol-basis-bot/config.yaml"
```

## Deployment Options

### Option 1: Bare Metal / VPS

Best for: Maximum control, lowest latency

```bash
# Install dependencies
sudo apt update
sudo apt install build-essential pkg-config libssl-dev

# Build and deploy
cargo build --release
sudo cp target/release/sol-basis-bot /opt/sol-basis-bot/
sudo systemctl restart sol-basis-bot
```

### Option 2: Docker

```dockerfile
# Dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/sol-basis-bot /usr/local/bin/
COPY config.yaml /etc/sol-basis-bot/
CMD ["sol-basis-bot", "--config", "/etc/sol-basis-bot/config.yaml"]
```

```bash
docker build -t sol-basis-bot .
docker run -d --name sol-basis-bot \
  -v /path/to/keypair.json:/keys/keypair.json:ro \
  -v /path/to/config.yaml:/etc/sol-basis-bot/config.yaml:ro \
  -e SOLANA_KEYPAIR_PATH=/keys/keypair.json \
  -p 9090:9090 \
  sol-basis-bot
```

### Option 3: Kubernetes

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sol-basis-bot
spec:
  replicas: 1  # Only 1 instance!
  selector:
    matchLabels:
      app: sol-basis-bot
  template:
    metadata:
      labels:
        app: sol-basis-bot
    spec:
      containers:
      - name: bot
        image: sol-basis-bot:latest
        resources:
          requests:
            memory: "2Gi"
            cpu: "1"
          limits:
            memory: "4Gi"
            cpu: "2"
        ports:
        - containerPort: 9090
        volumeMounts:
        - name: config
          mountPath: /etc/sol-basis-bot
        - name: keys
          mountPath: /keys
          readOnly: true
      volumes:
      - name: config
        configMap:
          name: sol-basis-bot-config
      - name: keys
        secret:
          secretName: sol-basis-bot-keypair
```

## Monitoring

### Prometheus Metrics

Scrape config:
```yaml
scrape_configs:
  - job_name: 'sol-basis-bot'
    static_configs:
      - targets: ['localhost:9090']
```

### Key Metrics to Monitor

| Metric | Alert Threshold |
|--------|-----------------|
| `sol_basis_bot_agent_state` | != 1 (Idle) for > 1h without position |
| `sol_basis_bot_error_count` | > 10 / hour |
| `sol_basis_bot_drawdown_pct` | > 3% |
| `sol_basis_bot_unrealized_pnl` | < -$100 |
| `sol_basis_bot_rpc_latency_ms` | > 500ms |

### Grafana Dashboard

Import the provided dashboard from `monitoring/grafana-dashboard.json`.

### Alerts

Configure alerts in `config.yaml`:
```yaml
telemetry:
  alert_webhook_url: "https://hooks.slack.com/..."
  telegram_bot_token: "..."
  telegram_chat_id: "..."
```

## Security

### Keypair Security

1. **Never commit keypairs to git**
2. Use file permissions: `chmod 600 keypair.json`
3. Consider hardware wallets for large funds
4. Use separate keypairs for testing vs production

### Network Security

1. Firewall: Only allow metrics port (9090) from monitoring
2. Use private RPC endpoints
3. Enable TLS for all external connections

### Operational Security

1. Start with paper trading
2. Use small positions initially
3. Set conservative risk limits
4. Monitor 24/7 for first week
5. Have kill switch ready

## Troubleshooting

### Common Issues

**Bot not connecting to RPC**
```bash
# Check RPC health
curl -X POST https://your-rpc.com -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}'
```

**High latency**
- Check network path to RPC
- Consider co-located infrastructure
- Reduce update intervals

**Transactions failing**
- Check SOL balance for fees
- Increase priority fees
- Check Jito bundle status

**Position drift**
- Check rebalance settings
- Verify both legs executing
- Check slippage settings

### Log Analysis

```bash
# View live logs
journalctl -u sol-basis-bot -f

# Search for errors
journalctl -u sol-basis-bot | grep ERROR

# Export logs
journalctl -u sol-basis-bot --since "1 hour ago" > debug.log
```

### Emergency Procedures

1. **Stop the bot**
   ```bash
   sudo systemctl stop sol-basis-bot
   ```

2. **Close positions manually**
   - Use Drift UI to close perp
   - Swap spot back to USDC via Jupiter

3. **Investigate**
   - Check logs for errors
   - Verify wallet balances
   - Check protocol status

## Support

- GitHub Issues: https://github.com/bmwtx3/sol-basis-bot/issues
- Documentation: https://github.com/bmwtx3/sol-basis-bot/wiki
