//! # state
//!
//! The Antigravity **shared application state** — the single source of truth
//! that both the Brain Loop (writes) and Reflex Loop (reads) share.
//!
//! ## Design Decisions
//!
//! * `Arc<AppState>` is cloned cheaply into every Axum handler via
//!   `axum::extract::State`.
//! * `RwLock<Option<ActiveStrategy>>` allows *many concurrent readers* (Reflex
//!   Loop ticks) with *exclusive writer access* (Brain Loop updates).
//! * We deliberately avoid `Mutex` here: a `Mutex` would serialise all tick
//!   reads, which is unacceptable at high tick frequency.
//!
//! ## Thread‑Safety Guarantee
//!
//! The `RwLock` from `tokio::sync` is async-aware, so neither readers nor the
//! single writer ever block an OS thread — they yield cooperatively to the
//! Tokio runtime.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::ActiveStrategy;

// ─── AppState ─────────────────────────────────────────────────────────────────

/// Top-level shared state injected into every Axum handler.
///
/// Clone this via `Arc::clone` — the `Arc` wrapper makes that O(1).
#[derive(Clone)]
pub struct AppState {
    /// The current trade plan published by the Brain Loop (OpenClaw).
    ///
    /// `None` means no strategy has been established yet; the Reflex Loop
    /// must **not** fire any trades in that case.
    pub active_strategy: Arc<RwLock<Option<ActiveStrategy>>>,

    /// Counter of how many ticks have been processed.  Useful for health-check
    /// dashboards and detecting MT5 disconnects (counter stalls).
    pub tick_count: Arc<std::sync::atomic::AtomicU64>,

    /// Counter of how many trade execution commands have been fired this
    /// session.  Monotonically increasing.
    pub trade_count: Arc<std::sync::atomic::AtomicU64>,
}

impl AppState {
    /// Construct a fresh, empty application state.
    pub fn new() -> Self {
        Self {
            active_strategy: Arc::new(RwLock::new(None)),
            tick_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            trade_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience type alias so callers can write `SharedState` instead of the
/// full generic form.
pub type SharedState = Arc<AppState>;

/// Construct the shared application state and wrap it in an `Arc` ready for
/// injection into the Axum router.
pub fn build_state() -> SharedState {
    Arc::new(AppState::new())
}
