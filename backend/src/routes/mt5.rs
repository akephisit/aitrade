//! # routes::mt5
//!
//! Axum route handlers สำหรับ MetaTrader 5 interface (Reflex Loop)

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::atomic::Ordering;
use tracing::error;

use crate::{
    engine::{
        executor::{build_order, fire_trade},
        reflex::{evaluate_tick, TradeSignal},
    },
    error::AppError,
    events::WsEvent,
    models::{
        position::{OpenPosition, TradeRecord, TradeStatus},
        Direction, TickData,
    },
    risk::RiskDecision,
    state::SharedState,
};

// ─── POST /api/mt5/tick ───────────────────────────────────────────────────────

/// **Reflex Loop entry point** — รับ Tick จาก MT5, ประเมิน, ยิง Trade (ถ้าถึงเวลา)
pub async fn handle_tick(
    State(state): State<SharedState>,
    Json(tick): Json<TickData>,
) -> Result<impl IntoResponse, AppError> {
    // ── 1. Reflex Engine ──────────────────────────────────────────────────────
    let signal = evaluate_tick(&tick, &state).await?;

    match signal {
        // ── No Action — Fast path (ส่วนใหญ่จะผ่านทางนี้) ─────────────────────
        TradeSignal::NoAction => Ok((
            StatusCode::OK,
            Json(json!({
                "ok":     true,
                "action": "NO_ACTION",
                "symbol": tick.symbol,
                "bid":    tick.bid,
                "ask":    tick.ask,
            })),
        )),

        // ── Trade Triggered ───────────────────────────────────────────────────
        TradeSignal::Trigger(strategy) => {
            // ── 2. Risk Check ────────────────────────────────────────────────────────────
            match state.risk.pre_trade_check().await {
                RiskDecision::Blocked(reason) => {
                    return Ok((
                        StatusCode::OK,
                        Json(json!({
                            "ok":     false,
                            "action": "RISK_BLOCKED",
                            "reason": reason,
                        })),
                    ));
                }
                RiskDecision::Approved => {}
            }

            // ── 3. Entry price ────────────────────────────────────────────────────────────
            let entry_price = match strategy.direction {
                Direction::Buy  => tick.ask,
                Direction::Sell => tick.bid,
                Direction::NoTrade => tick.effective_mid(),
            };

            // ── 3. Build MT5 order ────────────────────────────────────────────
            let order = build_order(
                &strategy.symbol,
                strategy.direction,
                entry_price,
                strategy.stop_loss,
                strategy.take_profit,
                strategy.lot_size,
                strategy.strategy_id,
            )?;

            // ── 4. สร้าง TradeRecord (สถานะ Pending) ──────────────────────────
            let mut record = TradeRecord::from_strategy(&strategy, entry_price);

            // ── 5. Broadcast "กำลังยิง Trade" ─────────────────────────────────
            state.broadcast(&WsEvent::TradeFiring {
                record: Box::new(record.clone()),
            });

            // ── 6. ล้าง ActiveStrategy ก่อน I/O ──────────────────────────────
            //    ป้องกัน Tick ที่เข้ามาระหว่างรอ MT5ตอบ trigger ซ้ำ
            {
                let mut guard = state.active_strategy.write().await;
                *guard = None;
            }

            // ── 7. ยิง Order จริงไป MT5 ───────────────────────────────────────
            let mt5_url = std::env::var("MT5_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8081".to_string());

            match fire_trade(&order, &state.http_client, &mt5_url).await {
                Ok(mt5_resp) => {
                    // ── 7a. SUCCESS ───────────────────────────────────────────
                    let ticket = mt5_resp.order;
                    record.status         = TradeStatus::Confirmed;
                    record.mt5_ticket     = ticket;
                    record.status_message = mt5_resp.comment
                        .unwrap_or_else(|| "Request completed".to_string());

                    // เปิด Position ใน State
                    let mut position = OpenPosition::from_strategy(&strategy, entry_price);
                    position.mt5_ticket = ticket;

                    state.set_open_position(Some(position.clone())).await;
                    state.push_trade_record(record.clone()).await;
                    state.risk.record_success().await;  // ✅ Reset consecutive failures

                    // Broadcast
                    state.broadcast(&WsEvent::PositionOpened {
                        position: Box::new(position.clone()),
                    });

                    Ok((
                        StatusCode::OK,
                        Json(json!({
                            "ok":          true,
                            "action":      "TRADE_TRIGGERED",
                            "strategy_id": strategy.strategy_id,
                            "trade_id":    record.trade_id,
                            "symbol":      strategy.symbol,
                            "direction":   strategy.direction,
                            "entry_price": entry_price,
                            "tp":          strategy.take_profit,
                            "sl":          strategy.stop_loss,
                            "mt5_ticket":  ticket,
                        })),
                    ))
                }

                Err(e) => {
                    // ── 7b. FAILED ────────────────────────────────────────────
                    error!(error = %e, "Trade execution failed");

                    record.status         = TradeStatus::Failed;
                    record.status_message = e.to_string();

                    state.push_trade_record(record.clone()).await;
                    state.risk.record_failure().await;  // ❌ Increment consecutive failures
                    state.broadcast(&WsEvent::TradeFailed {
                        record: Box::new(record),
                    });

                    Err(e)
                }
            }
        }
    }
}

// ─── POST /api/mt5/position-close ────────────────────────────────────────────
//
// MT5 EA เรียก endpoint นี้เมื่อ Position ถูกปิด (TP / SL / Manual)
// ทำให้ open_position = None → Double-Entry Protection รีเซ็ต → พร้อม Trade ใหม่

#[derive(serde::Deserialize)]
pub struct PositionClosePayload {
    pub mt5_ticket:   Option<u64>,
    pub symbol:       String,
    pub close_price:  f64,
    pub profit_pips:  f64,
    /// "TP" | "SL" | "MANUAL"
    pub close_reason: String,
}

pub async fn handle_position_close(
    State(state): State<SharedState>,
    Json(payload): Json<PositionClosePayload>,
) -> impl IntoResponse {
    // ดึง open_position ก่อน clear
    let current_pos = {
        let guard = state.open_position.read().await;
        guard.clone()
    };

    if let Some(pos) = current_pos {
        // 1. Clear open position → Reflex Loop พร้อม Trade ใหม่
        state.set_open_position(None).await;

        // 2. อัปเดต TradeRecord ใน History ด้วยข้อมูล Close
        {
            let mut history = state.trade_history.write().await;
            if let Some(record) = history.iter_mut()
                .find(|r| r.mt5_ticket == payload.mt5_ticket || r.symbol == payload.symbol)
            {
                record.close_price  = Some(payload.close_price);
                record.profit_pips  = Some(payload.profit_pips);
                record.close_reason = Some(payload.close_reason.clone());
                record.closed_at    = Some(chrono::Utc::now());
            }
        }

        // 3. Broadcast → Dashboard อัปเดต Real-time
        state.broadcast(&WsEvent::PositionClosed {
            position_id:  pos.position_id,
            symbol:       pos.symbol.clone(),
            direction:    format!("{:?}", pos.direction).to_uppercase(),
            close_price:  payload.close_price,
            profit_pips:  payload.profit_pips,
            close_reason: payload.close_reason.clone(),
        });

        tracing::info!(
            symbol       = %pos.symbol,
            close_price  = payload.close_price,
            profit_pips  = payload.profit_pips,
            close_reason = %payload.close_reason,
            "✅ Position closed — Reflex Loop re-armed"
        );

        Json(serde_json::json!({
            "ok":          true,
            "message":     "Position closed",
            "symbol":      pos.symbol,
            "close_price": payload.close_price,
            "profit_pips": payload.profit_pips,
            "close_reason": payload.close_reason,
        }))
    } else {
        tracing::warn!("position-close called but no open position found");
        Json(serde_json::json!({
            "ok":     false,
            "message": "No open position to close",
        }))
    }
}

// ─── GET /api/mt5/health ──────────────────────────────────────────────────────

pub async fn health_check(State(state): State<SharedState>) -> impl IntoResponse {
    let tick_count   = state.tick_count.load(Ordering::Relaxed);
    let trade_count  = state.trade_count.load(Ordering::Relaxed);
    let has_strategy = state.active_strategy.read().await.is_some();
    let has_position = state.open_position.read().await.is_some();

    Json(json!({
        "ok":           true,
        "tick_count":   tick_count,
        "trade_count":  trade_count,
        "has_strategy": has_strategy,
        "has_position": has_position,
    }))
}
