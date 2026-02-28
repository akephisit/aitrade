//! # engine::reflex
//!
//! The **Reflex Engine** — the hot path that runs on every incoming tick.
//!
//! This module contains the pure, side-effect-free evaluation logic that decides
//! *whether* a trade should fire.  The actual HTTP call to MT5 is dispatched
//! from here but lives in `engine::executor` to keep concerns separated.
//!
//! ## Performance Contract
//!
//! * `evaluate_tick` must complete in **< 1 µs** on average.
//! * It holds the `RwLock` read guard only long enough to clone the strategy,
//!   then releases it before any I/O.
//! * All branching is O(1) — no heap allocation in the hot path.

use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::models::{ActiveStrategy, Direction, TickData};
use crate::state::SharedState;

// ─── Trade Signal ─────────────────────────────────────────────────────────────

/// Result returned by `evaluate_tick`.
#[derive(Debug, PartialEq)]
pub enum TradeSignal {
    /// The tick is within the entry zone — caller should fire a trade.
    Trigger(Box<ActiveStrategy>),
    /// No action required this tick.
    NoAction,
}

// ─── Core Evaluation ──────────────────────────────────────────────────────────

/// Evaluate one tick against the current `ActiveStrategy`.
///
/// # Arguments
/// * `tick`  — the freshly received market tick.
/// * `state` — shared Axum state (holds the `RwLock`-protected strategy).
///
/// # Returns
/// * `Ok(TradeSignal::Trigger(strategy))` — price entered the zone; fire trade.
/// * `Ok(TradeSignal::NoAction)`          — tick outside zone, or no strategy.
/// * `Err(AppError::Internal(_))`         — unexpected lock-poisoning (should never happen with Tokio).
pub async fn evaluate_tick(
    tick: &TickData,
    state: &SharedState,
) -> Result<TradeSignal, AppError> {
    // ── 1. Increment the global tick counter ─────────────────────────────────
    state.tick_count.fetch_add(1, Ordering::Relaxed);

    // ── 2. Read-lock the strategy — released at end of this block ────────────
    let maybe_strategy = {
        let guard = state.active_strategy.read().await;
        guard.clone() // Clone is cheap (all fields are small / Arc-wrapped)
    }; // <── RwLock read guard dropped here; IO can now proceed freely

    // ── 3. Guard: no strategy installed yet ──────────────────────────────────
    let strategy = match maybe_strategy {
        Some(s) => s,
        None => {
            debug!(symbol = %tick.symbol, bid = tick.bid, ask = tick.ask,
                   "No active strategy — tick skipped");
            return Ok(TradeSignal::NoAction);
        }
    };

    // ── 4. Guard: strategy symbol must match tick symbol ─────────────────────
    if strategy.symbol != tick.symbol {
        debug!(
            strategy_symbol = %strategy.symbol,
            tick_symbol     = %tick.symbol,
            "Symbol mismatch — tick skipped"
        );
        return Ok(TradeSignal::NoAction);
    }

    // ── 5. Guard: strategy must not be expired ───────────────────────────────
    if !strategy.is_valid() {
        warn!(strategy_id = %strategy.strategy_id, "Strategy expired — skipping tick");
        return Ok(TradeSignal::NoAction);
    }

    // ── 6. Guard: direction must be actionable ───────────────────────────────
    if strategy.direction == Direction::NoTrade {
        return Ok(TradeSignal::NoAction);
    }

    // ── 7. Pick the correct price side for entry evaluation ──────────────────
    //
    //   BUY  → we pay the ASK (broker's offer price).  We want price to dip
    //           into the zone so we can buy cheaply — use `ask`.
    //   SELL → we receive the BID (broker's buy price).  We want price to rally
    //           into the zone so we can sell high — use `bid`.
    let entry_price = match strategy.direction {
        Direction::Buy => tick.ask,
        Direction::Sell => tick.bid,
        Direction::NoTrade => unreachable!(), // handled above
    };

    // ── 8. Zone check ─────────────────────────────────────────────────────────
    if strategy.entry_zone.contains(entry_price) {
        info!(
            strategy_id  = %strategy.strategy_id,
            symbol       = %tick.symbol,
            direction    = ?strategy.direction,
            entry_price  = entry_price,
            zone_low     = strategy.entry_zone.low,
            zone_high    = strategy.entry_zone.high,
            "🎯 ENTRY ZONE HIT — triggering trade"
        );

        state.trade_count.fetch_add(1, Ordering::Relaxed);
        return Ok(TradeSignal::Trigger(Box::new(strategy)));
    }

    // ── 9. No trigger ────────────────────────────────────────────────────────
    debug!(
        entry_price = entry_price,
        zone        = ?strategy.entry_zone,
        "Tick outside entry zone"
    );
    Ok(TradeSignal::NoAction)
}
