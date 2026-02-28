//! # events
//!
//! Defines [`WsEvent`] — ทุก Event ที่ระบบ Broadcast ออกไปผ่าน WebSocket
//! ไปยัง SvelteKit Monitor Loop
//!
//! ใช้ `tokio::sync::broadcast::Sender<String>` โดยแปลง WsEvent เป็น JSON
//! String ก่อนส่ง เพื่อหลีกเลี่ยง Clone constraints ที่ซับซ้อน

use serde::Serialize;

use crate::models::ActiveStrategy;
use crate::models::position::{OpenPosition, TradeRecord};

/// Event ทุกรูปแบบที่ SvelteKit Dashboard จะได้รับแบบ Real-time
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WsEvent {
    /// OpenClaw ส่ง Strategy ใหม่มา — Reflex Loop ถูก Armed แล้ว
    StrategyUpdated {
        strategy: Box<ActiveStrategy>,
    },

    /// Strategy ถูกล้างออก — Reflex Loop Disarmed
    StrategyCleared,

    /// Reflex Loop จับ Entry Zone ได้ → กำลังยิง Order
    TradeFiring {
        record: Box<TradeRecord>,
    },

    /// MT5 ยืนยัน Order แล้ว — Position เปิดอยู่
    PositionOpened {
        position: Box<OpenPosition>,
    },

    /// MT5 ปฏิเสธหรือส่งไม่ถึง
    TradeFailed {
        record: Box<TradeRecord>,
    },

    /// MT5 ปิด Position แล้ว (TP / SL / Manual)
    PositionClosed {
        position_id:  uuid::Uuid,
        symbol:       String,
        direction:    String,
        close_price:  f64,
        profit_pips:  f64,
        close_reason: String,   // "TP" | "SL" | "MANUAL"
    },

    /// Risk Kill Switch ถูกเปิด (ไม่ว่าจาก Auto-Kill หรือ Manual)
    RiskKilled {
        reason: String,
    },

    /// สถิติ Server (ส่งทุก N tick เพื่อให้ Dashboard ยัง alive)
    ServerStats {
        tick_count:   u64,
        trade_count:  u64,
        has_position: bool,
        has_strategy: bool,
    },
}

impl WsEvent {
    /// แปลงเป็น JSON String สำหรับส่งผ่าน WebSocket
    #[inline]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| r#"{"event":"SERIALIZATION_ERROR"}"#.to_string())
    }
}
