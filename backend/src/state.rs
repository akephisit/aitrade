//! # state
//!
//! AppState ที่ขยายแล้ว — รองรับ Position Management, Trade History,
//! WebSocket Broadcast Channel และ shared HTTP Client

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::engine::confirmation::{ConfirmationConfig, RecentTick};
use crate::engine::candle_builder::Candle;
use crate::models::{ActiveStrategy, OpenPosition, TradeRecord};
use crate::risk::{RiskConfig, RiskManager};

/// จำนวน Tick ที่เก็บ History ต่อ Symbol
const TICK_BUFFER_SIZE: usize = 30;

// ─── AppState ─────────────────────────────────────────────────────────────────

/// Top-level shared state injected into every Axum handler.
#[derive(Clone)]
pub struct AppState {
    // ── Brain Loop ────────────────────────────────────────────────────────────
    /// แผนการเทรดปัจจุบันจาก OpenClaw
    /// None = ยังไม่มีแผน หรือ แผนถูกล้างหลังจาก Trade fired
    pub active_strategy: Arc<RwLock<Option<ActiveStrategy>>>,

    // ── Position Management ───────────────────────────────────────────────────
    /// Position ที่เปิดอยู่ใน MT5 ณ ตอนนี้
    /// None = ไม่มี Position เปิด → Reflex Loop พร้อม trade
    /// Some = มี Position อยู่แล้ว → ห้าม Double Entry
    pub open_position: Arc<RwLock<Option<OpenPosition>>>,

    // ── Trade History ─────────────────────────────────────────────────────────
    /// บันทึกทุก Order ที่เคยยิง (ไม่มีวันลบ — ใช้สำหรับ Dashboard)
    /// ในอนาคตจะ persist ลง PostgreSQL
    pub trade_history: Arc<RwLock<Vec<TradeRecord>>>,

    // ── Monitor / WebSocket ───────────────────────────────────────────────────
    /// Broadcast channel สำหรับส่ง Event ไปยัง WebSocket clients
    /// ใช้ String (pre-serialized JSON) เพื่อหลีกเลี่ยง Clone constraints
    pub broadcast_tx: broadcast::Sender<String>,

    // ── HTTP Client ───────────────────────────────────────────────────────────
    /// reqwest Client ที่ share กันทั้งระบบ (thread-safe, connection pooling)
    /// สร้างครั้งเดียว ไม่ต้องสร้างใหม่ทุก Request
    pub http_client: reqwest::Client,

    // ── Metrics ───────────────────────────────────────────────────────────────
    pub tick_count:  Arc<std::sync::atomic::AtomicU64>,
    pub trade_count: Arc<std::sync::atomic::AtomicU64>,

    // ── Tick Buffer (Confirmation Engine) ────────────────────────────────────
    /// เก็บ Tick ย้อนหลังต่อ Symbol สำหรับ Zone Probe และ Dwell detection
    /// Key = symbol string, Value = ล่าสุดอยู่ท้าย VecDeque
    pub tick_buffer: Arc<RwLock<HashMap<String, VecDeque<RecentTick>>>>,

    // ── Candle Builder (M1 Rejection Engine) ──────────────────────────────────
    /// เก็บแท่งเทียนที่กำลังสร้างจาก Tick
    pub latest_candle: Arc<RwLock<HashMap<String, Candle>>>,

    // ── Confirmation Config ───────────────────────────────────────────────────
    pub confirmation_config: Arc<ConfirmationConfig>,

    // ── Risk Management ─────────────────────────────────────────────────
    pub risk: Arc<RiskManager>,
}

impl AppState {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);

        Self {
            active_strategy:     Arc::new(RwLock::new(None)),
            open_position:       Arc::new(RwLock::new(None)),
            trade_history:       Arc::new(RwLock::new(Vec::new())),
            broadcast_tx,
            http_client:         reqwest::Client::new(),
            tick_count:          Arc::new(std::sync::atomic::AtomicU64::new(0)),
            trade_count:         Arc::new(std::sync::atomic::AtomicU64::new(0)),
            tick_buffer:         Arc::new(RwLock::new(HashMap::new())),
            latest_candle:       Arc::new(RwLock::new(HashMap::new())),
            confirmation_config: Arc::new(ConfirmationConfig::from_env()),
            risk:                Arc::new(RiskManager::new(RiskConfig::from_env())),
        }
    }

    // ── Helper Methods ────────────────────────────────────────────────────────

    /// Broadcast WsEvent ไปยัง WebSocket clients ทั้งหมด
    /// ไม่ panic ถ้าไม่มี listener (ปลอดภัยสำหรับ headless mode)
    pub fn broadcast(&self, event: &crate::events::WsEvent) {
        // Err เกิดขึ้นเมื่อไม่มี receiver — ไม่ใช่ error จริงๆ
        let _ = self.broadcast_tx.send(event.to_json());
    }

    /// เพิ่ม TradeRecord เข้า history
    pub async fn push_trade_record(&self, record: TradeRecord) {
        let mut history = self.trade_history.write().await;
        history.push(record);
    }

    /// อัปเดต open_position (None = ปิด Position แล้ว)
    pub async fn set_open_position(&self, position: Option<OpenPosition>) {
        let mut guard = self.open_position.write().await;
        *guard = position;
    }

    /// เช็คว่ามี open position สำหรับ symbol นี้ไหม
    pub async fn has_open_position_for(&self, symbol: &str) -> bool {
        let guard = self.open_position.read().await;
        guard.as_ref().map(|p| p.symbol == symbol).unwrap_or(false)
    }

    /// บันทึก Tick ลง Buffer สำหรับ Confirmation Engine
    /// เรียกทุก Tick ก่อน Reflex evaluation
    pub async fn record_tick(&self, symbol: &str, bid: f64, ask: f64) {
        let mut buffer = self.tick_buffer.write().await;
        let entry = buffer
            .entry(symbol.to_string())
            .or_insert_with(|| VecDeque::with_capacity(TICK_BUFFER_SIZE + 1));

        if entry.len() >= TICK_BUFFER_SIZE {
            entry.pop_front();  // ลบ Tick เก่าสุด
        }
        entry.push_back(RecentTick::new(bid, ask));

        // ── สร้างหรืออัปเดตแท่งเทียน (M1) ──────────────────────────────────────────
        let mut candles = self.latest_candle.write().await;
        let mid_price = (bid + ask) / 2.0;
        let now = chrono::Utc::now();
        
        let candle = candles.entry(symbol.to_string()).or_insert_with(|| {
            Candle::new(symbol, now, mid_price)
        });

        // ถ้าเข้าสู่นาทีใหม่ เริ่มแท่งใหม่
        if now.timestamp() / 60 > candle.start_time.timestamp() / 60 {
            *candle = Candle::new(symbol, now, mid_price);
        } else {
            candle.update(mid_price);
        }
    }

    /// อ่าน Tick Buffer ของ symbol (clone ออกมาเพื่อปล่อย lock)
    pub async fn get_tick_buffer(&self, symbol: &str) -> VecDeque<RecentTick> {
        let buffer = self.tick_buffer.read().await;
        buffer.get(symbol).cloned().unwrap_or_default()
    }

    /// อ่านแท่งเทียนล่าสุด
    pub async fn get_latest_candle(&self, symbol: &str) -> Option<Candle> {
        let candles = self.latest_candle.read().await;
        candles.get(symbol).cloned()
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}

/// Convenience type alias
pub type SharedState = Arc<AppState>;

pub fn build_state() -> SharedState {
    Arc::new(AppState::new())
}
