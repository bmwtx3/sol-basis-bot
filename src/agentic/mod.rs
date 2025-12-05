//! Agentic Module
//!
//! Self-learning and adaptive features:
//! - Performance database (SQLite trade logging)
//! - Adaptive position sizing (Kelly criterion)
//! - Funding reversal detection

pub mod performance_db;
pub mod adaptive_sizing;
pub mod reversal_detector;

pub use performance_db::{PerformanceDb, TradeOutcome, PerformanceMetrics};
pub use adaptive_sizing::{AdaptiveSizer, SizingRecommendation};
pub use reversal_detector::{ReversalDetector, ReversalAlert, ReversalSeverity};
