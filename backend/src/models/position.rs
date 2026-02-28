//! # models::position
//!
//! Defines structs for tracking **live positions** and **trade history**.
//!
//! ## Why separate from ActiveStrategy?
//! `ActiveStrategy` = คำสั่งจาก AI (แผนล่วงหน้า)
//! `OpenPosition`   = Position ที่เปิดอยู่จริงใน MT5 แล้ว
//! `TradeRecord`    = Log ประวัติทุก Order ที่เคยยิง

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{ActiveStrategy, Direction};

// ─── TradeStatus ──────────────────────────────────────────────────────────────

/// สถานะของ Order ที่ยิงไป MT5
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeStatus {
    /// Order ถูกส่งไปแล้ว รอ MT5 ยืนยัน
    Pending,
    /// MT5 รับ Order แล้ว ได้ Ticket number กลับมา
    Confirmed,
    /// MT5 ปฏิเสธ Order (retcode ไม่ใช่ 10009)
    Rejected,
    /// ส่งไม่ถึง MT5 เลย (network error / timeout)
    Failed,
}

// ─── OpenPosition ─────────────────────────────────────────────────────────────

/// Position ที่กำลังเปิดอยู่ใน MT5 ณ ตอนนี้
///
/// ใช้ตรวจสอบก่อน Reflex Loop จะยิง Order ใหม่ —
/// ถ้ามี `OpenPosition` อยู่แล้ว → ห้ามเปิดซ้ำ (Double Entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    /// ID ภายในของ Position นี้
    pub position_id: Uuid,
    /// Strategy ที่สร้าง Position นี้
    pub strategy_id: Uuid,
    pub symbol: String,
    pub direction: Direction,
    pub entry_price: f64,
    pub lot_size: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
    /// Ticket number จาก MT5 (มีหลังจาก Confirmed เท่านั้น)
    pub mt5_ticket: Option<u64>,
    pub opened_at: DateTime<Utc>,
    /// สถานะเลื่อน SL วิ่งตามไปบังทุน (Break-Even) ทำไปแล้วหรือยัง?
    pub sl_moved_to_be: bool,
}

impl OpenPosition {
    pub fn from_strategy(strategy: &ActiveStrategy, entry_price: f64) -> Self {
        Self {
            position_id: Uuid::new_v4(),
            strategy_id: strategy.strategy_id,
            symbol: strategy.symbol.clone(),
            direction: strategy.direction,
            entry_price,
            lot_size: strategy.lot_size,
            take_profit: strategy.take_profit,
            stop_loss: strategy.stop_loss,
            mt5_ticket: None,
            opened_at: Utc::now(),
            sl_moved_to_be: false,
        }
    }

    /// คาดเดา Unrealised PnL จากราคาปัจจุบัน (ใช้โดย Dashboard)
    #[allow(dead_code)]
    pub fn unrealised_pips(&self, current_price: f64) -> f64 {
        match self.direction {
            Direction::Buy  => current_price - self.entry_price,
            Direction::Sell => self.entry_price - current_price,
            Direction::NoTrade => 0.0,
        }
    }
}

// ─── TradeRecord ──────────────────────────────────────────────────────────────

/// บันทึกประวัติของ Order แต่ละรายการ — ไม่มีวันลบ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: Uuid,
    pub strategy_id: Uuid,
    pub symbol: String,
    pub direction: Direction,
    pub entry_price: f64,
    pub lot_size: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
    /// Ticket number จาก MT5 (ถ้า Confirmed)
    pub mt5_ticket:     Option<u64>,
    pub status:         TradeStatus,
    /// ข้อความจาก MT5 หรือ error message
    pub status_message: String,
    pub fired_at:       DateTime<Utc>,
    // ── ข้อมูลตอนปิด Position (เพิ่มเมื่อ MT5 แจ้ง close) ────────────────────
    pub close_price:    Option<f64>,
    pub profit_pips:    Option<f64>,
    pub close_reason:   Option<String>,  // "TP" | "SL" | "MANUAL"
    pub closed_at:      Option<DateTime<Utc>>,
}

impl TradeRecord {
    /// สร้าง TradeRecord เริ่มต้นจาก Strategy (สถานะ Pending)
    pub fn from_strategy(strategy: &ActiveStrategy, entry_price: f64) -> Self {
        Self {
            trade_id:       Uuid::new_v4(),
            strategy_id:    strategy.strategy_id,
            symbol:         strategy.symbol.clone(),
            direction:      strategy.direction,
            entry_price,
            lot_size:       strategy.lot_size,
            take_profit:    strategy.take_profit,
            stop_loss:      strategy.stop_loss,
            mt5_ticket:     None,
            status:         TradeStatus::Pending,
            status_message: "Order queued".to_string(),
            fired_at:       Utc::now(),
            close_price:    None,
            profit_pips:    None,
            close_reason:   None,
            closed_at:      None,
        }
    }
}
