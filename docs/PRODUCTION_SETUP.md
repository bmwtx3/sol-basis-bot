# Production Setup Guide

Complete guide for running the SOL Basis Trading Bot in production.

## 1. Lowest Latency Location to Host

### Recommended: Frankfurt, Germany 🏆

**Why Frankfurt?**
- Frankfurt hosts the highest concentration of validators globally
- Frankfurt achieves sub-millisecond latency to Jito-Solana, enabling maximum MEV capture
- Frankfurt has 0.6ms latency to Jito

### Alternative Locations (Ranked)

| Location | Latency to Jito | Notes |
|----------|-----------------|-------|
| **Frankfurt** | ~0.6-0.9ms | Best overall, highest validator density |
| **London** | ~0.2ms | Excellent EU alternative |
| **New York/New Jersey** | ~0.05-0.3ms | Best for US operations |
| **Amsterdam** | ~1.2-1.7ms | Good EU backup |
| **Tokyo** | ~3-5ms | Best for Asia |
| **Singapore** | ~5-10ms | Asia alternative |

### Hosting Providers

**Recommended for Solana:**

1. **DedicatedNodes.io** - Solana-optimized bare metal
   - Frankfurt: €999-1499/month
   - 10Gbps, Solana traffic included
   - AMD EPYC, 1TB+ NVMe, 1024GB RAM

2. **Hivelocity** - Enterprise bare metal
   - Multiple locations worldwide
   - Custom Solana configurations
   - 24/7 support

3. **AVORO/Dataforest** - Frankfurt specialist
   - Sub-millisecond Jito latency
   - Pre-optimized for Solana

4. **HOSTKEY** - Budget-friendly option
   - Amsterdam & Frankfurt
   - From ~$400/month

---

## 2. Fastest, Highest Capacity RPC (Affordable)

### RPC Provider Comparison

| Provider | Tier | Price | RPS | Best For |
|----------|------|-------|-----|----------|
| **Helius** (Recommended) | Business | $399/mo | 500 | Trading bots, production |
| **Helius** | Professional | $999/mo | 1000+ | High-frequency |
| **QuickNode** | Build | $49/mo | 25 | Development |
| **Chainstack** | Growth | $299/mo | 300 | Multi-chain |
| **Triton One** | Tier 1 | $500/mo | 50 | Ultra-low latency |

### Our Recommendation: Helius Business Plan

**Price:** $399/month
**Includes:**
- 400M credits/month
- 500 RPS
- Staked connections (auto-enabled)
- Enhanced WebSockets
- Helius provides some of the fastest Solana RPC nodes, with latency ranging between 10–80 milliseconds

**Why Helius?**
- At $200/month or less your best bet is probably Helius. QuickNode is cheaper but somewhat less performant. Triton is the gold standard, but is outside your budget.
- Solana-native, optimized for trading
- Staked connections improve transaction landing
- Built-in Jito integration

### Budget Options

**Starting out ($50-200/month):**
```yaml
# config.yaml - Budget setup
rpc:
  endpoints:
    - url: "https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
      weight: 100
    - url: "https://solana-mainnet.g.alchemy.com/v2/YOUR_KEY"  # Backup
      weight: 30
```

**Production ($400-1000/month):**
```yaml
# config.yaml - Production setup
rpc:
  endpoints:
    - url: "https://mainnet.helius-rpc.com/?api-key=YOUR_HELIUS_KEY"
      weight: 100
    - url: "https://YOUR_TRITON_ENDPOINT"
      weight: 50
```

---

## 3. Recommended Risk Management Values

### Conservative (Recommended for Starting)

```yaml
risk:
  max_drawdown_pct: 3.0          # Pause at 3% loss from peak
  stop_loss_pct: 1.0             # Close at 1% position loss
  hedge_drift_threshold_pct: 1.5 # Rebalance at 1.5% drift
  max_funding_reversal_loss: 100 # Max $100 daily loss

trading:
  min_basis_spread_pct: 0.15     # Only trade 0.15%+ basis
  min_funding_apr_pct: 20.0      # Only trade 20%+ APR
  max_leverage: 1.5              # Conservative leverage
  max_position_size_sol: 100.0   # Small positions
  min_trade_interval_secs: 300   # 5 min between trades

rebalance:
  check_interval_secs: 120       # Check every 2 min
  min_rebalance_size_sol: 5.0    # Minimum 5 SOL
  max_rebalances_per_hour: 4     # Max 4 per hour
```

### Moderate (After 1 Month Successful Paper Trading)

```yaml
risk:
  max_drawdown_pct: 5.0
  stop_loss_pct: 2.0
  hedge_drift_threshold_pct: 2.0
  max_funding_reversal_loss: 300

trading:
  min_basis_spread_pct: 0.10
  min_funding_apr_pct: 15.0
  max_leverage: 2.0
  max_position_size_sol: 500.0
  min_trade_interval_secs: 120

rebalance:
  check_interval_secs: 60
  min_rebalance_size_sol: 10.0
  max_rebalances_per_hour: 6
```

### Aggressive (Experienced Only)

```yaml
risk:
  max_drawdown_pct: 7.0
  stop_loss_pct: 3.0
  hedge_drift_threshold_pct: 3.0
  max_funding_reversal_loss: 500

trading:
  min_basis_spread_pct: 0.08
  min_funding_apr_pct: 12.0
  max_leverage: 3.0
  max_position_size_sol: 1000.0
  min_trade_interval_secs: 60

rebalance:
  check_interval_secs: 30
  min_rebalance_size_sol: 20.0
  max_rebalances_per_hour: 10
```

### Key Risk Principles

1. **Never risk more than you can afford to lose**
2. **Start with paper trading for at least 2 weeks**
3. **Start with 10% of intended capital**
4. **Increase position size only after proving profitability**

---

## 4. Infrastructure & Environment Variables

### Required Environment Variables

```bash
# Required - Wallet keypair path
export SOLANA_KEYPAIR_PATH="/path/to/your/keypair.json"

# Optional - Override config location
export SOL_BASIS_BOT_CONFIG="/opt/sol-basis-bot/config.yaml"

# Optional - Log level
export RUST_LOG="sol_basis_bot=info,solana_client=warn"

# Optional - Disable colors in logs
export NO_COLOR=1
```

### Required Infrastructure

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 2 cores, 2.8GHz+ | 4+ cores, 3.5GHz+ |
| RAM | 4 GB | 8 GB |
| Storage | 20 GB SSD | 50 GB NVMe |
| Network | 100 Mbps | 1 Gbps |
| OS | Ubuntu 22.04+ | Ubuntu 24.04 |

### Required Accounts/Keys

1. **Solana Wallet**
   ```bash
   # Generate new keypair
   solana-keygen new --outfile ~/sol-basis-bot-wallet.json
   
   # Or use existing - NEVER use your main wallet!
   ```

2. **RPC Provider Account**
   - Sign up at https://www.helius.dev
   - Get API key from dashboard
   - Add to config.yaml

3. **Drift Protocol Account** (for perps)
   - Visit https://app.drift.trade
   - Connect wallet
   - Deposit collateral (USDC)

### Network Requirements

- Outbound HTTPS (443) to RPC endpoints
- Outbound WSS (443) for WebSocket
- Inbound 9090 (optional, for Prometheus metrics)

---

## 5. Instructions to Run

### Step 1: Server Setup

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install dependencies
sudo apt install -y build-essential pkg-config libssl-dev curl git

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Solana CLI (optional, for key management)
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
```

### Step 2: Clone and Build

```bash
# Clone repository
git clone https://github.com/bmwtx3/sol-basis-bot.git
cd sol-basis-bot

# Build release binary
cargo build --release

# Verify build
./target/release/sol-basis-bot --help
```

### Step 3: Configure

```bash
# Copy and edit config
cp config.yaml config.prod.yaml
nano config.prod.yaml
```

Edit these key values:
```yaml
paper_trading: true  # START WITH TRUE!

rpc:
  endpoints:
    - url: "https://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY"
      weight: 100

execution:
  use_jito: true
  jito_tip_lamports: 10000
```

### Step 4: Set Environment

```bash
# Create environment file
cat > ~/.sol-basis-bot.env << 'EOF'
export SOLANA_KEYPAIR_PATH="/home/your_user/sol-basis-bot-wallet.json"
export RUST_LOG="sol_basis_bot=info"
EOF

# Load environment
source ~/.sol-basis-bot.env
```

### Step 5: Run Paper Trading (REQUIRED FIRST)

```bash
# Run in paper trading mode
./target/release/sol-basis-bot --config config.prod.yaml --paper

# Or with explicit flag in config
# paper_trading: true
./target/release/sol-basis-bot --config config.prod.yaml
```

### Step 6: Monitor

```bash
# View logs
tail -f /path/to/logs/bot.log

# Check metrics (if enabled)
curl http://localhost:9090/metrics
```

### Step 7: Production (After Successful Paper Trading)

```bash
# Edit config
nano config.prod.yaml
# Change: paper_trading: false

# Run with systemd (recommended)
sudo cp sol-basis-bot.service /etc/systemd/system/
sudo systemctl enable sol-basis-bot
sudo systemctl start sol-basis-bot

# Check status
sudo systemctl status sol-basis-bot
```

### Quick Start Commands

```bash
# Development/testing
cargo run --release -- --paper

# Paper trading (production-like)
./target/release/sol-basis-bot --config config.yaml --paper

# Live trading (CAREFUL!)
./target/release/sol-basis-bot --config config.yaml

# With custom log level
RUST_LOG=debug ./target/release/sol-basis-bot --config config.yaml --paper
```

---

## 6. Google Sheets Webhook Integration

Yes! You can hook into Google Sheets for transaction reporting. Here's how:

### Option 1: Google Apps Script Webhook (Recommended)

**Step 1: Create Google Apps Script**

1. Open your Google Sheet
2. Extensions → Apps Script
3. Paste this code:

```javascript
function doPost(e) {
  const sheet = SpreadsheetApp.getActiveSpreadsheet().getActiveSheet();
  const data = JSON.parse(e.postData.contents);
  
  // Add row with transaction data
  sheet.appendRow([
    new Date(),                    // Timestamp
    data.type,                     // Trade type (open/close/rebalance)
    data.side,                     // Buy/Sell
    data.size,                     // Size in SOL
    data.price,                    // Price
    data.pnl || 0,                 // P&L
    data.basis_spread,             // Basis at time of trade
    data.funding_apr,              // Funding APR
    data.signature || '',          // Transaction signature
    data.status                    // Success/Failed
  ]);
  
  return ContentService.createTextOutput(JSON.stringify({status: 'ok'}))
    .setMimeType(ContentService.MimeType.JSON);
}
```

4. Deploy → New Deployment → Web App
5. Execute as: Me, Who has access: Anyone
6. Copy the webhook URL

**Step 2: Add to Bot Config**

```yaml
telemetry:
  enable_metrics: true
  metrics_port: 9090
  
  # Add webhook for Google Sheets
  alert_webhook_url: "https://script.google.com/macros/s/YOUR_SCRIPT_ID/exec"
```

**Step 3: Modify Alert Module**

Add to `src/telemetry/alerts.rs`:

```rust
// Add trade reporting
pub async fn report_trade(
    webhook_url: &str,
    trade_type: &str,
    side: &str,
    size: f64,
    price: f64,
    pnl: f64,
    basis_spread: f64,
    funding_apr: f64,
    signature: Option<&str>,
    status: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    
    let payload = serde_json::json!({
        "type": trade_type,
        "side": side,
        "size": size,
        "price": price,
        "pnl": pnl,
        "basis_spread": basis_spread,
        "funding_apr": funding_apr,
        "signature": signature.unwrap_or(""),
        "status": status,
    });
    
    client.post(webhook_url)
        .json(&payload)
        .send()
        .await?;
    
    Ok(())
}
```

### Option 2: Zapier/Make Integration

1. Create a Zap: Webhook → Google Sheets
2. Get Zapier webhook URL
3. Add to config:

```yaml
telemetry:
  alert_webhook_url: "https://hooks.zapier.com/hooks/catch/YOUR_ZAP_ID"
```

### Option 3: Direct Google Sheets API

```rust
// Add to Cargo.toml
// google-sheets4 = "5"

// In your code
use google_sheets4::Sheets;
// ... implement direct API calls
```

### Sample Google Sheet Headers

| Timestamp | Type | Side | Size (SOL) | Price | P&L | Basis % | Funding APR % | Signature | Status |
|-----------|------|------|------------|-------|-----|---------|---------------|-----------|--------|
| 2024-12-05 10:30:00 | open | long_spot | 100 | 148.52 | 0 | 0.25 | 18.42 | abc123... | success |
| 2024-12-05 10:30:01 | open | short_perp | 100 | 148.89 | 0 | 0.25 | 18.42 | def456... | success |
| 2024-12-05 14:45:00 | close | sell_spot | 100 | 149.10 | 58.00 | 0.05 | 15.20 | ghi789... | success |

---

## Quick Reference

### Startup Checklist

- [ ] Server in Frankfurt (or low-latency location)
- [ ] Helius Business plan ($399/mo) or equivalent RPC
- [ ] Fresh Solana wallet (NOT your main wallet)
- [ ] Funded with SOL for fees (~1 SOL minimum)
- [ ] Drift account with USDC collateral
- [ ] Config file with conservative settings
- [ ] Paper trading for 2+ weeks
- [ ] Monitoring/alerting configured
- [ ] Google Sheets webhook (optional)

### Cost Estimate (Monthly)

| Item | Cost |
|------|------|
| Server (Frankfurt bare metal) | $400-1000 |
| Helius Business RPC | $399 |
| Jito tips (variable) | $50-200 |
| **Total** | **$850-1600/month** |

### Expected Returns

Conservative estimate with $10,000 capital:
- Average funding APR captured: 15-20%
- After fees and costs: 10-15% net APR
- Monthly: $80-125 profit
- Yearly: $1,000-1,500 profit

**Note:** Returns vary significantly based on market conditions. Basis trading is NOT guaranteed profit.

---

## Support

- GitHub Issues: https://github.com/bmwtx3/sol-basis-bot/issues
- Helius Discord: https://discord.gg/helius
- Solana Discord: https://discord.gg/solana
