//! Benchmarks for performance-critical paths
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::atomic::{AtomicU64, Ordering};

/// AtomicF64 wrapper for benchmarking
struct AtomicF64(AtomicU64);

impl AtomicF64 {
    fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }
    
    fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
    
    fn store(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Release);
    }
}

/// Benchmark atomic price updates
fn bench_atomic_price_update(c: &mut Criterion) {
    let price = AtomicF64::new(150.0);
    
    c.bench_function("atomic_price_update", |b| {
        b.iter(|| {
            price.store(black_box(150.12345));
            black_box(price.load())
        })
    });
}

/// Benchmark basis spread calculation
fn bench_basis_calculation(c: &mut Criterion) {
    c.bench_function("basis_spread_calc", |b| {
        b.iter(|| {
            let spot = black_box(150.0);
            let perp = black_box(150.30);
            black_box(((perp - spot) / spot) * 100.0)
        })
    });
}

/// Benchmark funding APR calculation
fn bench_funding_apr(c: &mut Criterion) {
    c.bench_function("funding_apr_calc", |b| {
        b.iter(|| {
            let rate = black_box(0.0001);
            black_box(rate * 3.0 * 365.0 * 100.0)
        })
    });
}

/// Benchmark statistics calculation
fn bench_statistics(c: &mut Criterion) {
    let data: Vec<f64> = (0..100).map(|i| 150.0 + (i as f64) * 0.01).collect();
    
    c.bench_function("mean_calculation", |b| {
        b.iter(|| {
            let sum: f64 = black_box(&data).iter().sum();
            black_box(sum / data.len() as f64)
        })
    });
    
    c.bench_function("std_dev_calculation", |b| {
        b.iter(|| {
            let data = black_box(&data);
            let mean: f64 = data.iter().sum::<f64>() / data.len() as f64;
            let variance: f64 = data.iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>() / data.len() as f64;
            black_box(variance.sqrt())
        })
    });
}

/// Benchmark percentile calculation
fn bench_percentile(c: &mut Criterion) {
    let mut data: Vec<f64> = (0..1000).map(|i| 0.1 + (i as f64) * 0.001).collect();
    
    c.bench_function("percentile_calc", |b| {
        b.iter(|| {
            let mut sorted = black_box(data.clone());
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let value = black_box(0.25);
            let count = sorted.iter().filter(|&&x| x <= value).count();
            black_box((count as f64 / sorted.len() as f64) * 100.0)
        })
    });
}

/// Benchmark signal evaluation
fn bench_signal_evaluation(c: &mut Criterion) {
    c.bench_function("signal_evaluation", |b| {
        b.iter(|| {
            let basis = black_box(0.25);
            let funding_apr = black_box(20.0);
            let min_basis = black_box(0.10);
            let min_funding = black_box(15.0);
            
            let mut confidence = 0.0;
            
            if basis >= min_basis {
                confidence += 0.3;
            }
            if funding_apr >= min_funding {
                confidence += 0.3;
            }
            if (basis > 0.0) == (funding_apr > 0.0) {
                confidence += 0.2;
            }
            
            black_box(confidence)
        })
    });
}

/// Benchmark position sizing
fn bench_position_sizing(c: &mut Criterion) {
    c.bench_function("position_sizing", |b| {
        b.iter(|| {
            let max_position = black_box(1000.0);
            let confidence = black_box(0.8);
            let spread = black_box(0.25);
            let min_spread = black_box(0.10);
            let funding = black_box(20.0);
            let min_funding = black_box(15.0);
            
            let base_size = max_position * 0.2;
            let spread_mult = (spread / min_spread).min(3.0);
            let funding_mult = (funding / min_funding).sqrt().min(2.0);
            
            black_box((base_size * spread_mult * funding_mult * confidence).min(max_position))
        })
    });
}

/// Benchmark drawdown calculation
fn bench_drawdown(c: &mut Criterion) {
    c.bench_function("drawdown_calc", |b| {
        b.iter(|| {
            let peak = black_box(10000.0);
            let current = black_box(9500.0);
            black_box(((peak - current) / peak) * 100.0)
        })
    });
}

/// Benchmark event processing throughput
fn bench_event_processing(c: &mut Criterion) {
    let events = vec![
        ("SpotPriceUpdate", 150.0),
        ("PerpMarkPriceUpdate", 150.30),
        ("FundingRateUpdate", 0.0001),
    ];
    
    c.bench_function("event_batch_processing", |b| {
        b.iter(|| {
            for (event_type, value) in black_box(&events) {
                match *event_type {
                    "SpotPriceUpdate" => black_box(*value),
                    "PerpMarkPriceUpdate" => black_box(*value),
                    "FundingRateUpdate" => black_box(*value),
                    _ => black_box(0.0),
                };
            }
        })
    });
}

/// Benchmark with different data sizes
fn bench_rolling_window(c: &mut Criterion) {
    let mut group = c.benchmark_group("rolling_window");
    
    for size in [100, 500, 1000, 5000].iter() {
        let data: Vec<f64> = (0..*size).map(|i| 150.0 + (i as f64) * 0.001).collect();
        
        group.bench_with_input(
            BenchmarkId::new("mean", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let sum: f64 = black_box(data).iter().sum();
                    black_box(sum / data.len() as f64)
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_atomic_price_update,
    bench_basis_calculation,
    bench_funding_apr,
    bench_statistics,
    bench_percentile,
    bench_signal_evaluation,
    bench_position_sizing,
    bench_drawdown,
    bench_event_processing,
    bench_rolling_window,
);

criterion_main!(benches);
