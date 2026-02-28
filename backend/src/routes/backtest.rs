//! # routes::backtest
//!
//! **Backtesting Engine** — ทดสอบ Strategy กับข้อมูลย้อนหลัง
//!
//! ## How it works
//! รับ Array ของ TickData + ActiveStrategy แล้วจำลอง Reflex + Confirmation Engine
//! คืน Statistics: Win Rate, PnL, Max Drawdown, Trade List
//!
//! ## Endpoint
//! POST /api/backtest

use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;

use crate::{
    engine::confirmation::{check_confirmation, ConfirmationConfig, RecentTick},
    models::{ActiveStrategy, Direction, TickData},
};

// ─── Request ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BacktestRequest {
    /// ชุดข้อมูล Tick ย้อนหลัง (เรียงตามเวลา เก่า → ใหม่)
    pub ticks:    Vec<TickData>,
    /// Strategy ที่ต้องการทดสอบ
    pub strategy: ActiveStrategy,
    /// Override Confirmation Config (ถ้าไม่ใส่ใช้ค่า default)
    pub confirmation: Option<ConfirmationOverride>,
}

#[derive(Deserialize)]
pub struct ConfirmationOverride {
    pub max_spread:         Option<f64>,
    pub require_zone_probe: Option<bool>,
    pub min_zone_ticks:     Option<usize>,
    pub probe_lookback:     Option<usize>,
}

// ─── Response ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BacktestResult {
    /// จำนวน Tick ที่ผ่าน
    pub total_ticks:    usize,
    /// จำนวน Trade ที่ถูก Trigger
    pub total_trades:   usize,
    /// กำไร/ขาดทุนรวม (pips) — สมมุติว่าทุก TP ถูก Hit
    pub total_pips:     f64,
    /// Win Rate % (TP ถูก Hit / Total Trades)
    pub win_rate_pct:   f64,
    /// Max Drawdown (pips) — ติดลบมากที่สุดในช่วง Simulation
    pub max_drawdown:   f64,
    /// รายการ Trade แต่ละ Entry
    pub trades:         Vec<BacktestTrade>,
    /// เหตุผลที่ไม่ Trigger (breakdown)
    pub rejection_log:  RejectionBreakdown,
}

#[derive(Debug, Serialize)]
pub struct BacktestTrade {
    pub entry_price: f64,
    pub direction:   String,
    pub outcome:     TradeOutcome,
    pub pips:        f64,
    pub tick_index:  usize,
    pub time:        chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, PartialEq)]
pub enum TradeOutcome {
    TpHit,   // TP ถูก Hit (Win)
    SlHit,   // SL ถูก Hit (Loss)
    Open,    // ยังเปิดอยู่ตอนจบ Simulation
}

#[derive(Debug, Serialize, Default)]
pub struct RejectionBreakdown {
    pub no_strategy:         usize,
    pub outside_zone:        usize,
    pub spread_too_wide:     usize,
    pub no_zone_probe:       usize,
    pub insufficient_dwell:  usize,
    pub position_open:       usize,
}

// ─── Backtest Handler ─────────────────────────────────────────────────────────

/// POST /api/backtest
pub async fn run_backtest(
    Json(req): Json<BacktestRequest>,
) -> impl IntoResponse {
    let result = simulate(req);
    Json(json!({ "ok": true, "result": result }))
}

// ─── Simulation Engine ────────────────────────────────────────────────────────

fn simulate(req: BacktestRequest) -> BacktestResult {
    let strategy = &req.strategy;

    // Build confirmation config
    let mut config = ConfirmationConfig::default();
    if let Some(ov) = &req.confirmation {
        if let Some(v) = ov.max_spread         { config.max_spread = v; }
        if let Some(v) = ov.require_zone_probe  { config.require_zone_probe = v; }
        if let Some(v) = ov.min_zone_ticks      { config.min_zone_ticks = v; }
        if let Some(v) = ov.probe_lookback      { config.probe_lookback = v; }
    }

    let mut tick_buffer: VecDeque<RecentTick> = VecDeque::with_capacity(30);
    let mut trades:      Vec<BacktestTrade>    = Vec::new();
    let mut rejections   = RejectionBreakdown::default();
    let mut open_pos:    Option<OpenSimPos>    = None;
    let mut running_pnl  = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    let mut peak_pnl     = 0.0_f64;

    for (i, tick) in req.ticks.iter().enumerate() {
        // Feed buffer
        if tick_buffer.len() >= 30 { tick_buffer.pop_front(); }
        tick_buffer.push_back(RecentTick::new(tick.bid, tick.ask));

        let entry_price = match strategy.direction {
            Direction::Buy  => tick.ask,
            Direction::Sell => tick.bid,
            Direction::NoTrade => { rejections.no_strategy += 1; continue; }
        };

        // Close open position if TP/SL hit
        if let Some(pos) = open_pos.take() {
            let outcome = check_exit(tick, &pos);
            let pips = match &outcome {
                TradeOutcome::TpHit => strategy.take_profit - pos.entry_price,
                TradeOutcome::SlHit => strategy.stop_loss   - pos.entry_price,
                TradeOutcome::Open  => { open_pos = Some(pos); continue; }
            };
            let pips = if pos.direction == "BUY" { pips } else { -pips };
            running_pnl += pips;
            let drawdown = peak_pnl - running_pnl;
            if drawdown > max_drawdown { max_drawdown = drawdown; }
            if running_pnl > peak_pnl { peak_pnl = running_pnl; }

            if let Some(last) = trades.last_mut() {
                last.outcome = outcome;
                last.pips    = pips;
            }
            continue;
        }

        // Symbol check
        if strategy.symbol != tick.symbol { continue; }
        if !strategy.is_valid()           { continue; }
        if strategy.direction == Direction::NoTrade { rejections.no_strategy += 1; continue; }

        // Zone check
        if !strategy.entry_zone.contains(entry_price) {
            rejections.outside_zone += 1;
            continue;
        }

        // Confirmation check
        use crate::engine::confirmation::ConfirmationResult;
        match check_confirmation(tick.bid, tick.ask, &strategy.entry_zone, strategy.direction, &tick_buffer, None, tick.rsi_14, &config) {
            ConfirmationResult::Rejected { reason } => {
                match reason {
                    "spread too wide"        => rejections.spread_too_wide += 1,
                    "no zone probe detected" => rejections.no_zone_probe += 1,
                    "insufficient zone dwell"=> rejections.insufficient_dwell += 1,
                    _                        => {}
                }
                continue;
            }
            ConfirmationResult::Confirmed => {
                let dir_str = format!("{:?}", strategy.direction).to_uppercase();
                open_pos = Some(OpenSimPos {
                    entry_price,
                    direction:   dir_str.clone(),
                    take_profit: strategy.take_profit,
                    stop_loss:   strategy.stop_loss,
                });
                trades.push(BacktestTrade {
                    entry_price,
                    direction: dir_str,
                    outcome:   TradeOutcome::Open,
                    pips:      0.0,
                    tick_index: i,
                    time:      tick.time,
                });
            }
        }
    }

    // Close any remaining open position as "Open"
    // (already pushed as Open above)

    let total_trades = trades.len();
    let wins         = trades.iter().filter(|t| t.outcome == TradeOutcome::TpHit).count();
    let total_pips   = trades.iter().map(|t| t.pips).sum();
    let win_rate_pct = if total_trades > 0 {
        (wins as f64 / total_trades as f64) * 100.0
    } else { 0.0 };

    BacktestResult {
        total_ticks:  req.ticks.len(),
        total_trades,
        total_pips,
        win_rate_pct,
        max_drawdown,
        trades,
        rejection_log: rejections,
    }
}

struct OpenSimPos {
    entry_price: f64,
    direction:   String,
    take_profit: f64,
    stop_loss:   f64,
}

/// ตรวจว่า Tick ปัจจุบัน Hit TP หรือ SL หรือยัง
fn check_exit(tick: &TickData, pos: &OpenSimPos) -> TradeOutcome {
    if pos.direction == "BUY" {
        if tick.bid >= pos.take_profit { return TradeOutcome::TpHit; }
        if tick.bid <= pos.stop_loss   { return TradeOutcome::SlHit; }
    } else {
        if tick.ask <= pos.take_profit { return TradeOutcome::TpHit; }
        if tick.ask >= pos.stop_loss   { return TradeOutcome::SlHit; }
    }
    TradeOutcome::Open
}
