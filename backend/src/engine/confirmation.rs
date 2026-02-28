//! # engine::confirmation
//!
//! **Confirmation Engine** — ตรวจสอบ 3 ชั้นก่อนยิง Order
//!
//! ## ทำไมถึงต้องมี Confirmation?
//!
//! แค่ "ราคาอยู่ใน Zone" ไม่พอ เพราะ:
//! - ราคาอาจวิ่งทะลุ Zone ไปเลย (False Entry)
//! - อาจเป็นช่วงข่าว Spread กว้าง (High Risk)  
//! - อาจเป็นแค่ Wick ผ่านไปชั่วขณะ (Fake Touch)
//!
//! ## 3 ชั้นการตรวจสอบ
//!
//! ```text
//! ราคาเข้า Zone
//!     │
//!     ├─ [1] Spread Check   → ป้องกันช่วง High Volatility / News
//!     │
//!     ├─ [2] Zone Probe     → ราคาเคย "สัมผัส" นอก Zone ก่อนไหม?
//!     │      BUY:  เคยต่ำกว่า zone_low  → แสดงว่า Support ถูก Test แล้ว
//!     │      SELL: เคยสูงกว่า zone_high → แสดงว่า Resistance ถูก Reject แล้ว
//!     │
//!     └─ [3] Zone Dwell     → อยู่ใน Zone ต่อเนื่อง ≥ N ticks
//!            ป้องกัน Wick ผ่านชั่วขณะ
//! ```

use std::collections::VecDeque;
use tracing::debug;

use crate::models::{Direction, strategy::EntryZone};

// ─── Config ───────────────────────────────────────────────────────────────────

/// ค่า Config สำหรับ Confirmation Engine
/// อ่านจาก Environment Variables ผ่าน `ConfirmationConfig::from_env()`
#[derive(Debug, Clone)]
pub struct ConfirmationConfig {
    /// Spread สูงสุดที่ยอมรับได้ (หน่วยเดียวกับราคา)
    /// เช่น BTCUSD: 50.0 = $50 | EURUSD: 0.0003 = 3 pips
    pub max_spread: f64,

    /// ต้องมี Zone Probe ก่อนถึงจะเข้าไหม?
    /// true  = ต้องเห็นราคาทดสอบ นอก Zone ก่อน (แนะนำ)
    /// false = เข้าทันทีที่ราคาอยู่ใน Zone
    pub require_zone_probe: bool,

    /// ราคาต้องอยู่ใน Zone ต่อเนื่องกี่ Ticks ขั้นต่ำ
    /// ป้องกัน Wick/Spike ผ่าน Zone ชั่วคราว
    /// แนะนำ: 2-5 ticks
    pub min_zone_ticks: usize,

    /// ดู Tick ย้อนหลังกี่อันเพื่อหา Zone Probe
    /// ถ้า max ต่ำเกิน อาจไม่เจอ Probe (ชะลอการเข้า)
    /// แนะนำ: 10-20 ticks
    pub probe_lookback: usize,

    // ── [4] RSI Filter ─────────────────────────────────────────────────────
    /// RSI ที่เรียกว่า Overbought (สำหรับ BUY: ทางเปิดเมื่อ RSI < overbought)
    /// ถ้า TickData ไม่เห็น rsi_14 → ข้าม RSI check
    pub rsi_overbought: f64,

    /// RSI ที่เรียกว่า Oversold (สำหรับ SELL: ทางเปิดเมื่อ RSI > oversold)
    pub rsi_oversold: f64,
}

impl ConfirmationConfig {
    pub fn from_env() -> Self {
        Self {
            max_spread:        std::env::var("CONFIRM_MAX_SPREAD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(50.0),
            require_zone_probe: std::env::var("CONFIRM_REQUIRE_PROBE")
                .map(|v| v != "false" && v != "0").unwrap_or(true),
            min_zone_ticks:    std::env::var("CONFIRM_MIN_ZONE_TICKS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(2),
            probe_lookback:    std::env::var("CONFIRM_PROBE_LOOKBACK")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(15),
            rsi_overbought:    std::env::var("CONFIRM_RSI_OVERBOUGHT")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(70.0),
            rsi_oversold:      std::env::var("CONFIRM_RSI_OVERSOLD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(30.0),
        }
    }
}

impl Default for ConfirmationConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

// ─── Recent Tick (Compact) ────────────────────────────────────────────────────

/// ข้อมูล Tick ที่ย่อให้เล็กที่สุด สำหรับเก็บใน Buffer
/// ไม่เก็บ String (symbol) เพราะ Buffer แยกตาม Symbol อยู่แล้ว
#[derive(Debug, Clone, Copy)]
pub struct RecentTick {
    pub mid:    f64,
    pub spread: f64,
}

impl RecentTick {
    pub fn new(bid: f64, ask: f64) -> Self {
        Self {
            mid:    (bid + ask) / 2.0,
            spread: ask - bid,
        }
    }
}

// ─── Result ───────────────────────────────────────────────────────────────────

/// ผลการตรวจสอบ Confirmation
#[derive(Debug, PartialEq)]
pub enum ConfirmationResult {
    /// ผ่านทุกชั้น → ยิง Trade ได้
    Confirmed,
    /// ไม่ผ่าน — พร้อมบอกสาเหตุ (สำหรับ Log)
    Rejected { reason: &'static str },
}

// ─── Main Check ───────────────────────────────────────────────────────────────

/// ตรวจสอบ 4 ชั้น: Spread → Zone Probe → Zone Dwell → RSI
///
/// # Arguments
/// * `current_bid` / `current_ask` — ราคาปัจจุบัน
/// * `zone`     — Entry Zone จาก ActiveStrategy
/// * `dir`      — BUY หรือ SELL
/// * `buffer`   — Tick Buffer ย้อนหลัง (ล่าสุดอยู่ท้าย VecDeque)
/// * `rsi`      — RSI ปัจจุบัน (ส่ง None ถ้า MT5 ไม่คำนวณหรือไม่ส่งมา → ข้ามได้)
/// * `config`   — Confirmation parameters
pub fn check_confirmation(
    current_bid: f64,
    current_ask: f64,
    zone:        &EntryZone,
    dir:         Direction,
    buffer:      &VecDeque<RecentTick>,
    rsi:         Option<f64>,
    config:      &ConfirmationConfig,
) -> ConfirmationResult {
    let spread = current_ask - current_bid;
    let mid    = (current_bid + current_ask) / 2.0;

    // ── [1] Spread Check ──────────────────────────────────────────────────────
    if spread > config.max_spread {
        debug!(
            spread       = spread,
            max_spread   = config.max_spread,
            "❌ Confirmation REJECTED: spread too wide"
        );
        return ConfirmationResult::Rejected { reason: "spread too wide" };
    }

    // ── [2] Zone Probe Check ──────────────────────────────────────────────────
    // ตรวจว่าราคาเคย "สัมผัส" นอก Zone ก่อนที่จะกลับเข้ามาไหม
    if config.require_zone_probe {
        let lookback = buffer.len().min(config.probe_lookback);
        let recent   = buffer.iter().rev().take(lookback);

        // BUY:  ราคาเคยต่ำกว่า zone_low → "Support ถูก Test แล้วกลับมา" ✅
        // SELL: ราคาเคยสูงกว่า zone_high → "Resistance ถูก Reject แล้วกลับมา" ✅
        let probe_found = match dir {
            Direction::Buy  => recent.clone().any(|t| t.mid < zone.low),
            Direction::Sell => recent.clone().any(|t| t.mid > zone.high),
            Direction::NoTrade => false,
        };

        if !probe_found {
            debug!(
                direction   = ?dir,
                zone_low    = zone.low,
                zone_high   = zone.high,
                lookback,
                "❌ Confirmation REJECTED: no zone probe in recent ticks"
            );
            return ConfirmationResult::Rejected { reason: "no zone probe detected" };
        }

        debug!("✓ Zone probe confirmed");
    }

    // ── [3] Zone Dwell Check ──────────────────────────────────────────────────
    // นับ Ticks ที่อยู่ใน Zone ต่อเนื่องกัน (จากล่าสุดย้อนขึ้นไป)
    // ถ้าน้อยเกินไป = ราคาแค่ผ่าน Zone (Wick/Spike) ไม่ใช่ Price Action จริง
    let in_zone_consecutive = buffer
        .iter()
        .rev()                                              // นับจากล่าสุด
        .take_while(|t| zone.contains(t.mid))              // หยุดเมื่อออกนอก Zone
        .count();

    // บวก 1 สำหรับ Tick ปัจจุบัน (ซึ่งยังไม่ได้ push ลง buffer)
    let total_dwell = in_zone_consecutive + if zone.contains(mid) { 1 } else { 0 };

    if total_dwell < config.min_zone_ticks {
        debug!(
            dwell_ticks  = total_dwell,
            min_required = config.min_zone_ticks,
            "❌ Confirmation REJECTED: insufficient zone dwell"
        );
        return ConfirmationResult::Rejected { reason: "insufficient zone dwell" };
    }

    debug!(
        dwell_ticks = total_dwell,
        spread,
        "✅ Zone checks passed — checking RSI..."
    );

    // ── [4] RSI Filter (สามารถ Skip ได้ถ้าไม่ส่ง RSI) ───────────────────────────
    if let Some(rsi_val) = rsi {
        let blocked = match dir {
            // BUY: ห้ามเข้าเมื่อ Overbought (RSI สูง)
            Direction::Buy  => rsi_val >= config.rsi_overbought,
            // SELL: ห้ามเข้าเมื่อ Oversold (RSI ต่ำ)
            Direction::Sell => rsi_val <= config.rsi_oversold,
            Direction::NoTrade => false,
        };

        if blocked {
            debug!(
                rsi          = rsi_val,
                overbought   = config.rsi_overbought,
                oversold     = config.rsi_oversold,
                direction    = ?dir,
                "❌ Confirmation REJECTED: RSI out of range"
            );
            return ConfirmationResult::Rejected { reason: "rsi out of range" };
        }
        debug!(rsi = rsi_val, "✓ RSI check passed");
    } else {
        debug!("— RSI not available, skipping RSI check");
    }

    debug!(spread, "✅ All confirmations passed — FIRE!");
    ConfirmationResult::Confirmed
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_zone() -> EntryZone {
        EntryZone { low: 67000.0, high: 67050.0 }
    }

    fn make_config() -> ConfirmationConfig {
        ConfirmationConfig {
            max_spread:         50.0,
            require_zone_probe: true,
            min_zone_ticks:     2,
            probe_lookback:     10,
            rsi_overbought:     70.0,
            rsi_oversold:       30.0,
        }
    }

    fn make_buffer(mids: &[f64]) -> VecDeque<RecentTick> {
        mids.iter().map(|&m| RecentTick { mid: m, spread: 2.0 }).collect()
    }

    #[test]
    fn test_spread_too_wide() {
        let buffer = make_buffer(&[66990.0, 67020.0, 67025.0]);
        let result = check_confirmation(
            67020.0, 67080.0,  // spread = 60 > 50
            &make_zone(), Direction::Buy, &buffer, None, &make_config()
        );
        assert_eq!(result, ConfirmationResult::Rejected { reason: "spread too wide" });
    }

    #[test]
    fn test_no_zone_probe() {
        let buffer = make_buffer(&[67010.0, 67015.0, 67020.0]);
        let result = check_confirmation(
            67020.0, 67022.0,
            &make_zone(), Direction::Buy, &buffer, None, &make_config()
        );
        assert_eq!(result, ConfirmationResult::Rejected { reason: "no zone probe detected" });
    }

    #[test]
    fn test_confirmed_buy() {
        let buffer = make_buffer(&[66980.0, 66995.0, 67010.0, 67020.0]);
        let result = check_confirmation(
            67025.0, 67027.0,
            &make_zone(), Direction::Buy, &buffer, None, &make_config()
        );
        assert_eq!(result, ConfirmationResult::Confirmed);
    }

    #[test]
    fn test_confirmed_sell() {
        let buffer = make_buffer(&[67070.0, 67060.0, 67040.0, 67030.0]);
        let result = check_confirmation(
            67028.0, 67030.0,
            &make_zone(), Direction::Sell, &buffer, None, &make_config()
        );
        assert_eq!(result, ConfirmationResult::Confirmed);
    }

    #[test]
    fn test_insufficient_dwell() {
        let buffer = make_buffer(&[66985.0, 66990.0, 66999.0]);
        let result = check_confirmation(
            67005.0, 67007.0,
            &make_zone(), Direction::Buy, &buffer, None, &make_config()
        );
        assert_eq!(result, ConfirmationResult::Rejected { reason: "insufficient zone dwell" });
    }

    #[test]
    fn test_rsi_overbought_blocks_buy() {
        // RSI = 75 > 70 (overbought) → BUY ไม่ผ่าน
        let buffer = make_buffer(&[66980.0, 66995.0, 67010.0, 67020.0]);
        let result = check_confirmation(
            67025.0, 67027.0,
            &make_zone(), Direction::Buy, &buffer, Some(75.0), &make_config()
        );
        assert_eq!(result, ConfirmationResult::Rejected { reason: "rsi out of range" });
    }

    #[test]
    fn test_rsi_normal_allows_buy() {
        // RSI = 55 < 70 → ผ่าน
        let buffer = make_buffer(&[66980.0, 66995.0, 67010.0, 67020.0]);
        let result = check_confirmation(
            67025.0, 67027.0,
            &make_zone(), Direction::Buy, &buffer, Some(55.0), &make_config()
        );
        assert_eq!(result, ConfirmationResult::Confirmed);
    }
}
