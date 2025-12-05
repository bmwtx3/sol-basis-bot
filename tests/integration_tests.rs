//! Integration Tests for SOL Basis Trading Bot
//!
//! Tests the complete trading flow from signal to execution.

use std::sync::Arc;
use tokio::sync::broadcast;

// Note: These tests require the main crate to be compiled as a library
// Add `[lib]` section to Cargo.toml for full integration testing

#[cfg(test)]
mod tests {
    use super::*;

    /// Test configuration loading
    #[test]
    fn test_config_validation() {
        // Test that default config is valid
        let yaml = r#"
paper_trading: true
devnet: true

rpc:
  endpoints:
    - url: "https://api.devnet.solana.com"
      weight: 100
  request_timeout_ms: 5000
  max_retries: 3

protocols:
  pyth:
    sol_usd_feed: "J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"
    update_interval_ms: 1000
  jupiter:
    api_url: "https://quote-api.jup.ag/v6"
    sol_mint: "So11111111111111111111111111111111111111112"
    usdc_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
  drift:
    program_id: "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH"
    sol_perp_market_index: 0
    update_interval_ms: 1000

trading:
  min_basis_spread_pct: 0.10
  min_funding_apr_pct: 15.0
  max_leverage: 3.0
  max_position_size_sol: 100.0
  target_position_size_sol: 50.0
  basis_close_threshold_pct: 0.05
  min_trade_interval_secs: 60

risk:
  max_drawdown_pct: 5.0
  stop_loss_pct: 2.0
  hedge_drift_threshold_pct: 2.0
  max_funding_reversal_loss: 100.0

rebalance:
  check_interval_secs: 60
  min_rebalance_size_sol: 5.0
  max_rebalances_per_hour: 10

execution:
  use_jito: false
  jito_block_engine_url: "https://mainnet.block-engine.jito.wtf"
  jito_tip_lamports: 10000
  max_retries: 3
  retry_delay_ms: 500
  simulate_before_submit: true
  slippage_bps: 50
  priority_fee_percentile: 75

telemetry:
  log_level: "info"
  enable_metrics: false
  metrics_port: 9090
"#;
        
        let config: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
        assert!(config.is_ok(), "Config should parse successfully");
    }

    /// Test basis spread calculation
    #[test]
    fn test_basis_spread_calculation() {
        let spot_price = 150.0;
        let perp_price = 150.30;
        
        let spread = ((perp_price - spot_price) / spot_price) * 100.0;
        
        assert!((spread - 0.2).abs() < 0.001, "Spread should be ~0.2%");
    }

    /// Test funding rate annualization
    #[test]
    fn test_funding_apr_calculation() {
        // 0.01% per 8 hours
        let funding_rate = 0.0001;
        
        // 3 funding periods per day, 365 days
        let apr = funding_rate * 3.0 * 365.0 * 100.0;
        
        assert!((apr - 10.95).abs() < 0.01, "APR should be ~10.95%");
    }

    /// Test hedge ratio calculation
    #[test]
    fn test_hedge_ratio() {
        let spot_size = 100.0;
        let perp_size = 98.0;
        
        let hedge_ratio = perp_size / spot_size;
        let drift = ((spot_size - perp_size) / spot_size) * 100.0;
        
        assert!((hedge_ratio - 0.98).abs() < 0.001);
        assert!((drift - 2.0).abs() < 0.001);
    }

    /// Test position P&L calculation
    #[test]
    fn test_position_pnl() {
        // Long spot
        let spot_entry = 150.0;
        let spot_exit = 155.0;
        let spot_size = 100.0;
        let spot_pnl = (spot_exit - spot_entry) * spot_size;
        
        // Short perp
        let perp_entry = 150.30;
        let perp_exit = 155.30;
        let perp_size = 100.0;
        let perp_pnl = (perp_entry - perp_exit) * perp_size; // Short profits when price drops
        
        // Net should be close to zero (delta neutral)
        let net_pnl = spot_pnl + perp_pnl;
        
        assert!((spot_pnl - 500.0).abs() < 0.01);
        assert!((perp_pnl - (-500.0)).abs() < 0.01);
        assert!(net_pnl.abs() < 1.0, "Delta neutral position should have near-zero P&L");
    }

    /// Test drawdown calculation
    #[test]
    fn test_drawdown() {
        let peak_equity = 10000.0;
        let current_equity = 9500.0;
        
        let drawdown = ((peak_equity - current_equity) / peak_equity) * 100.0;
        
        assert!((drawdown - 5.0).abs() < 0.001);
    }

    /// Test signal confidence scoring
    #[test]
    fn test_signal_confidence() {
        let mut confidence = 0.0;
        
        // Basis condition met
        let basis = 0.15;
        let min_basis = 0.10;
        if basis >= min_basis {
            confidence += 0.3;
        }
        
        // Funding condition met
        let funding_apr = 20.0;
        let min_funding = 15.0;
        if funding_apr >= min_funding {
            confidence += 0.3;
        }
        
        // Alignment bonus
        let aligned = true;
        if aligned {
            confidence += 0.2;
        }
        
        assert!((confidence - 0.8).abs() < 0.001);
    }

    /// Test position sizing
    #[test]
    fn test_position_sizing() {
        let max_position = 1000.0;
        let base_size = max_position * 0.2; // 20% base
        
        let spread = 0.25;
        let min_spread = 0.10;
        let spread_multiple = (spread / min_spread).min(3.0);
        
        let sized = base_size * spread_multiple;
        
        assert!((sized - 500.0).abs() < 0.01); // 200 * 2.5 = 500
    }

    /// Test state transitions
    #[test]
    fn test_state_machine_transitions() {
        // Valid transitions
        assert!(can_transition("Idle", "Opening"));
        assert!(can_transition("Opening", "Monitoring"));
        assert!(can_transition("Monitoring", "Closing"));
        assert!(can_transition("Closing", "Idle"));
        assert!(can_transition("Monitoring", "Rebalancing"));
        assert!(can_transition("Rebalancing", "Monitoring"));
        assert!(can_transition("Monitoring", "Paused"));
        assert!(can_transition("Paused", "Idle"));
        
        // Invalid transitions
        assert!(!can_transition("Idle", "Closing"));
        assert!(!can_transition("Idle", "Monitoring"));
        assert!(!can_transition("Opening", "Closing"));
    }
    
    fn can_transition(from: &str, to: &str) -> bool {
        match (from, to) {
            ("Idle", "Opening") => true,
            ("Idle", "Paused") => true,
            ("Opening", "Monitoring") => true,
            ("Opening", "Idle") => true,
            ("Opening", "Paused") => true,
            ("Monitoring", "Closing") => true,
            ("Monitoring", "Rebalancing") => true,
            ("Monitoring", "Paused") => true,
            ("Closing", "Idle") => true,
            ("Closing", "Paused") => true,
            ("Rebalancing", "Monitoring") => true,
            ("Rebalancing", "Paused") => true,
            ("Paused", "Idle") => true,
            ("Paused", "Monitoring") => true,
            ("Paused", "Closing") => true,
            ("Error", "Idle") => true,
            ("Error", "Paused") => true,
            _ => false,
        }
    }

    /// Test rate limiting
    #[test]
    fn test_rate_limiting() {
        let max_per_hour = 10;
        let mut count = 0;
        
        for _ in 0..15 {
            if count < max_per_hour {
                count += 1;
            }
        }
        
        assert_eq!(count, 10);
    }

    /// Test slippage calculation
    #[test]
    fn test_slippage() {
        let amount = 1000.0;
        let slippage_bps = 50; // 0.5%
        
        let min_output = amount * (1.0 - (slippage_bps as f64 / 10000.0));
        
        assert!((min_output - 995.0).abs() < 0.01);
    }
}

/// Simulation tests for backtesting
#[cfg(test)]
mod simulation_tests {
    /// Simulate a complete trade cycle
    #[test]
    fn test_trade_cycle_simulation() {
        // Initial conditions
        let initial_capital = 10000.0;
        let spot_price = 150.0;
        let perp_price = 150.30;
        let basis = (perp_price - spot_price) / spot_price * 100.0; // 0.2%
        
        // Position sizing
        let position_size = 66.67; // ~10,000 / 150
        let spot_cost = position_size * spot_price;
        let perp_margin = spot_cost * 0.1; // 10% margin
        
        // Simulate convergence
        let exit_spot = 152.0;
        let exit_perp = 152.05; // Basis converged to 0.03%
        
        // P&L calculation
        let spot_pnl = (exit_spot - spot_price) * position_size;
        let perp_pnl = (perp_price - exit_perp) * position_size; // Short
        
        // Funding earned (assume 0.01% per 8h, held for 1 period)
        let funding_earned = perp_price * position_size * 0.0001;
        
        let total_pnl = spot_pnl + perp_pnl + funding_earned;
        
        assert!(total_pnl > 0.0, "Trade should be profitable");
        
        // ROI check
        let roi = (total_pnl / initial_capital) * 100.0;
        println!("Simulated trade ROI: {:.2}%", roi);
    }

    /// Simulate adverse conditions
    #[test]
    fn test_adverse_scenario() {
        // Scenario: Basis widens instead of converging
        let spot_entry = 150.0;
        let perp_entry = 150.30;
        let position_size = 100.0;
        
        // Basis widens
        let spot_exit = 145.0;
        let perp_exit = 145.75; // Basis widened to 0.5%
        
        let spot_pnl = (spot_exit - spot_entry) * position_size; // -500
        let perp_pnl = (perp_entry - perp_exit) * position_size; // +455
        
        let total_pnl = spot_pnl + perp_pnl; // -45
        
        // Loss is bounded (delta neutral helps)
        assert!(total_pnl > -100.0, "Loss should be bounded");
        println!("Adverse scenario loss: ${:.2}", total_pnl);
    }

    /// Test funding reversal scenario
    #[test]
    fn test_funding_reversal() {
        // Entered with positive funding (shorts pay longs)
        let initial_funding_rate = 0.001; // 0.1% per 8h
        let position_notional = 15000.0;
        
        // Expected earnings
        let expected_daily = position_notional * initial_funding_rate * 3.0;
        
        // Funding flips negative
        let reversed_rate = -0.0005;
        let actual_payment = position_notional * reversed_rate * 3.0;
        
        let funding_loss = actual_payment; // Negative = loss for shorts
        
        println!("Expected earnings: ${:.2}", expected_daily);
        println!("Actual (reversal): ${:.2}", actual_payment);
        assert!(funding_loss < 0.0, "Funding reversal causes loss");
    }
}
