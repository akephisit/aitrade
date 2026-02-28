//! # models::tick
//!
//! Defines [`TickData`], the raw market pulse that MetaTrader 5 sends to the
//! `/api/mt5/tick` endpoint on every price-update event.
//!
//! Keeping this struct minimal and `Copy`-able is intentional: the Reflex
//! Loop must deserialise and process thousands of ticks per second without heap
//! allocation overhead.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single price tick received from MetaTrader 5.
///
/// MT5 pushes this payload over HTTP POST (or WebSocket frame) every time the
/// market quote changes.  It mirrors the MQL5 `MqlTick` structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickData {
    /// The trading symbol, e.g. `"BTCUSD"`, `"EURUSD"`, `"NAS100"`.
    pub symbol: String,

    /// The current **bid** price (price at which market makers buy from us).
    pub bid: f64,

    /// The current **ask** price (price at which market makers sell to us).
    pub ask: f64,

    /// Mid-point convenience field: `(bid + ask) / 2`.
    /// MT5 can compute this client-side, or we derive it server-side.
    #[serde(default)]
    pub mid: Option<f64>,

    /// Volume traded at this tick (may be 0 for Forex quotes).
    pub volume: f64,

    /// Spread in points (ask − bid).
    #[serde(default)]
    pub spread: Option<f64>,

    /// UTC timestamp when MT5 recorded this tick.
    pub time: DateTime<Utc>,

    // ── Optional Indicators (MT5 EA ส่งมาให้ได้ถ้าคำนวณผ่าน iCustom()) ──────────
    /// RSI 14-period (0–100). ถ้าไม่ส่งมา Confirmation Engine จะข้าม RSI check
    #[serde(default)]
    pub rsi_14: Option<f64>,

    /// Moving Average 20-period
    #[serde(default)]
    pub ma_20: Option<f64>,

    /// Moving Average 50-period
    #[serde(default)]
    pub ma_50: Option<f64>,
}

impl TickData {
    /// Returns the effective mid price, computing it from bid/ask if the
    /// optional `mid` field was not provided by MT5.
    #[inline]
    pub fn effective_mid(&self) -> f64 {
        self.mid.unwrap_or_else(|| (self.bid + self.ask) / 2.0)
    }
}
