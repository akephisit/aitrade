//! # engine::reflex
//!
//! **Reflex Engine** — Hot path ที่รันทุก Tick
//! เพิ่มการตรวจสอบ Open Position เพื่อป้องกัน Double Entry

use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::models::{ActiveStrategy, Direction, TickData};
use crate::state::SharedState;

// ─── Trade Signal ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum TradeSignal {
    /// Price เข้า Entry Zone — caller ต้องยิง Trade
    Trigger(Box<ActiveStrategy>),
    /// ไม่มีอะไรต้องทำ Tick นี้
    NoAction,
}

// ─── Core Evaluation ──────────────────────────────────────────────────────────

/// ประเมิน 1 Tick เทียบกับ ActiveStrategy ปัจจุบัน
pub async fn evaluate_tick(
    tick: &TickData,
    state: &SharedState,
) -> Result<TradeSignal, AppError> {
    // ── 1. Increment tick counter ─────────────────────────────────────────────
    state.tick_count.fetch_add(1, Ordering::Relaxed);

    // ── 2. Clone strategy (release lock ทันทีก่อน I/O) ───────────────────────
    let maybe_strategy = {
        let guard = state.active_strategy.read().await;
        guard.clone()
    };

    let strategy = match maybe_strategy {
        Some(s) => s,
        None => {
            debug!(symbol = %tick.symbol, "No active strategy — tick skipped");
            return Ok(TradeSignal::NoAction);
        }
    };

    // ── 3. Guard: Symbol match ────────────────────────────────────────────────
    if strategy.symbol != tick.symbol {
        return Ok(TradeSignal::NoAction);
    }

    // ── 4. Guard: Strategy expiry ─────────────────────────────────────────────
    if !strategy.is_valid() {
        warn!(strategy_id = %strategy.strategy_id, "Strategy expired — skipping");
        return Ok(TradeSignal::NoAction);
    }

    // ── 5. Guard: Direction actionable ───────────────────────────────────────
    if strategy.direction == Direction::NoTrade {
        return Ok(TradeSignal::NoAction);
    }

    // ── 6. [NEW] Guard: Double Entry Protection ───────────────────────────────
    // ถ้ามี Position เปิดอยู่กับ Symbol เดียวกัน → ห้ามเปิดซ้ำ
    if state.has_open_position_for(&tick.symbol).await {
        debug!(symbol = %tick.symbol, "Position already open — double-entry blocked");
        return Ok(TradeSignal::NoAction);
    }

    // ── 7. ราคาที่ใช้ตรวจสอบ Entry Zone ─────────────────────────────────────
    //   BUY  → เราจ่าย Ask  (โบรกเกอร์คิดราคา Offer)
    //   SELL → เราได้ Bid   (โบรกเกอร์คิดราคา Bid)
    let entry_price = match strategy.direction {
        Direction::Buy  => tick.ask,
        Direction::Sell => tick.bid,
        Direction::NoTrade => unreachable!(),
    };

    // ── 8. Zone check ─────────────────────────────────────────────────────────
    if strategy.entry_zone.contains(entry_price) {
        info!(
            strategy_id = %strategy.strategy_id,
            symbol       = %tick.symbol,
            direction    = ?strategy.direction,
            entry_price,
            zone_low     = strategy.entry_zone.low,
            zone_high    = strategy.entry_zone.high,
            "🎯 ENTRY ZONE HIT — triggering trade"
        );

        state.trade_count.fetch_add(1, Ordering::Relaxed);
        return Ok(TradeSignal::Trigger(Box::new(strategy)));
    }

    debug!(entry_price, zone = ?strategy.entry_zone, "Tick outside entry zone");
    Ok(TradeSignal::NoAction)
}
