//! # models::strategy
//!
//! Defines [`ActiveStrategy`] — the "plan" object that the Brain Loop writes
//! (after consulting OpenClaw / Claude / GPT-4o) and the Reflex Loop reads on
//! every tick to decide whether to fire a trade execution command.
//!
//! Keeping this object small and `Clone`-able ensures the `RwLock` read guard
//! is held for the absolute minimum time inside the hot tick path.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Direction ────────────────────────────────────────────────────────────────

/// The AI's directional bias for the next trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    Buy,
    Sell,
    /// Neutral — OpenClaw sees no edge; Reflex Loop must not open trades.
    NoTrade,
}

// ─── EntryZone ────────────────────────────────────────────────────────────────

/// A price range in which the strategy authorises entry.
///
/// For a **Buy** the Reflex Loop triggers when price *drops into* the zone
/// (limit order semantics).  For a **Sell** it triggers when price *rises into*
/// the zone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntryZone {
    /// Lower bound of the acceptable entry range.
    pub low: f64,
    /// Upper bound of the acceptable entry range.
    pub high: f64,
}

impl EntryZone {
    /// Returns `true` if `price` falls inside `[low, high]`.
    #[inline]
    pub fn contains(&self, price: f64) -> bool {
        price >= self.low && price <= self.high
    }
}

// ─── ActiveStrategy ───────────────────────────────────────────────────────────

/// The complete trade plan written by OpenClaw and held in shared state.
///
/// This is intentionally **flat** — no nested heap allocations beyond
/// `String` — so a `RwLock` clone is fast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveStrategy {
    /// Unique identifier for this strategy "session".
    /// Useful for idempotency: MT5 can reject a duplicate order if the same
    /// `strategy_id` arrives twice (network retry scenario).
    pub strategy_id: Uuid,

    /// The symbol this strategy applies to, e.g. `"BTCUSD"`.
    pub symbol: String,

    /// AI's directional bias.
    pub direction: Direction,

    /// The price zone where the trade should be entered.
    pub entry_zone: EntryZone,

    /// Take-profit price level.
    pub take_profit: f64,

    /// Stop-loss price level.
    pub stop_loss: f64,

    /// โซนตรงข้าม (Supply/Demand ดักหน้า) ที่ใช้สำหรับระบบ Bailout (เผ่นก่อนชน)
    pub opposing_zone: Option<EntryZone>,

    /// Lot size / position size, e.g. `0.10` for 0.10 lots.
    pub lot_size: f64,

    /// Human-readable rationale from OpenClaw (for logging / UI display).
    pub rationale: String,

    /// UTC timestamp when OpenClaw produced this strategy.
    pub created_at: DateTime<Utc>,

    /// Optional expiry — the Reflex Loop should ignore this strategy after this
    /// timestamp to avoid stale signals.
    pub expires_at: Option<DateTime<Utc>>,
}

impl ActiveStrategy {
    /// Returns `true` if the strategy has not expired yet (or has no expiry).
    pub fn is_valid(&self) -> bool {
        match self.expires_at {
            Some(expiry) => Utc::now() < expiry,
            None => true,
        }
    }
}
