//! # engine::reflex
//!
//! **Reflex Engine** — Hot path ที่รันทุก Tick
//!
//! ## ลำดับการตรวจสอบ (ทุก Tick)
//! ```text
//! 1. Record tick into buffer   → ใช้โดย Confirmation Engine
//! 2. ตรวจ Strategy / Symbol / Expiry / Direction
//! 3. ตรวจ Double-Entry Protection
//! 4. ตรวจ Entry Zone (ราคาอยู่ใน Zone ไหม?)
//! 5. [NEW] Confirmation Engine:
//!    a. Spread Check  — Spread ปกติไหม?
//!    b. Zone Probe    — ราคาเคยทดสอบนอก Zone ก่อนไหม? (Bounce pattern)
//!    c. Zone Dwell    — ราคาอยู่ใน Zone ต่อเนื่องพอไหม?
//! 6. → TRIGGER trade
//! ```

use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

use crate::engine::confirmation::{check_confirmation, ConfirmationResult};
use crate::error::AppError;
use crate::models::{ActiveStrategy, Direction, TickData};
use crate::state::SharedState;

// ─── Trade Signal ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum TradeSignal {
    /// Price เข้า Zone + ผ่าน Confirmation → ยิง Trade
    Trigger(Box<ActiveStrategy>),
    /// ไม่มีอะไรต้องทำ Tick นี้
    NoAction,
}

// ─── Core Evaluation ──────────────────────────────────────────────────────────

pub async fn evaluate_tick(
    tick:  &TickData,
    state: &SharedState,
) -> Result<TradeSignal, AppError> {

    // ── 1. Record Tick into Buffer (ก่อนอื่นใดเลย) ────────────────────────────
    // ต้องทำก่อนทุก Guard เพราะ Buffer ต้องสะสม History แม้ในช่วงที่ไม่มี Strategy
    state.record_tick(&tick.symbol, tick.bid, tick.ask).await;

    // ── 2. Increment tick counter ─────────────────────────────────────────────
    state.tick_count.fetch_add(1, Ordering::Relaxed);

    // ── 3. Clone strategy (release lock ทันที) ────────────────────────────────
    let maybe_strategy = {
        let guard = state.active_strategy.read().await;
        guard.clone()
    };

    let strategy = match maybe_strategy {
        Some(s) => s,
        None => {
            debug!(symbol = %tick.symbol, "No active strategy — tick buffered only");
            return Ok(TradeSignal::NoAction);
        }
    };

    // ── 4. Guard: Symbol match ────────────────────────────────────────────────
    if strategy.symbol != tick.symbol {
        return Ok(TradeSignal::NoAction);
    }

    // ── 5. Guard: Strategy expiry ─────────────────────────────────────────────
    if !strategy.is_valid() {
        warn!(strategy_id = %strategy.strategy_id, "Strategy expired — skipping");
        return Ok(TradeSignal::NoAction);
    }

    // ── 6. Guard: Direction actionable ───────────────────────────────────────
    if strategy.direction == Direction::NoTrade {
        return Ok(TradeSignal::NoAction);
    }

    // ── 7. Guard: Double Entry ────────────────────────────────────────────────
    if state.has_open_position_for(&tick.symbol).await {
        debug!(symbol = %tick.symbol, "Position already open — double-entry blocked");
        return Ok(TradeSignal::NoAction);
    }

    // ── 8. Entry Price (ตาม Direction) ───────────────────────────────────────
    //   BUY  → จ่าย Ask (ราคาที่โบรกเกอร์ขายให้เรา)
    //   SELL → รับ Bid (ราคาที่โบรกเกอร์ซื้อจากเรา)
    let entry_price = match strategy.direction {
        Direction::Buy  => tick.ask,
        Direction::Sell => tick.bid,
        Direction::NoTrade => unreachable!(),
    };

    // ── 9. Zone Check ─────────────────────────────────────────────────────────
    if !strategy.entry_zone.contains(entry_price) {
        debug!(entry_price, zone = ?strategy.entry_zone, "Outside zone");
        return Ok(TradeSignal::NoAction);
    }

    // ─ ราคาอยู่ใน Zone แล้ว! → วิ่งไปหา Confirmation ──────────────────────────
    info!(
        strategy_id = %strategy.strategy_id,
        symbol      = %tick.symbol,
        direction   = ?strategy.direction,
        entry_price,
        zone_low    = strategy.entry_zone.low,
        zone_high   = strategy.entry_zone.high,
        "📍 Price in entry zone — running confirmation checks..."
    );

    // ── 10. [NEW] Confirmation Engine ────────────────────────────────────────
    let tick_buffer = state.get_tick_buffer(&tick.symbol).await;
    let config      = &*state.confirmation_config;

    let confirmation = check_confirmation(
        tick.bid,
        tick.ask,
        &strategy.entry_zone,
        strategy.direction,
        &tick_buffer,
        tick.rsi_14,      // ← ส่ง RSI จาก TickData (ถ้า None → ข้าม RSI check)
        config,
    );

    match confirmation {
        ConfirmationResult::Rejected { reason } => {
            debug!(
                reason,
                entry_price,
                "⏳ In zone but waiting for confirmation: {reason}"
            );
            return Ok(TradeSignal::NoAction);
        }

        ConfirmationResult::Confirmed => {
            info!(
                strategy_id = %strategy.strategy_id,
                symbol      = %tick.symbol,
                direction   = ?strategy.direction,
                entry_price,
                spread      = tick.ask - tick.bid,
                "🎯 CONFIRMED — firing trade!"
            );

            state.trade_count.fetch_add(1, Ordering::Relaxed);
            Ok(TradeSignal::Trigger(Box::new(strategy)))
        }
    }
}
