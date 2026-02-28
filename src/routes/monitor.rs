//! # routes::monitor
//!
//! **Monitor Loop** — Endpoints สำหรับ SvelteKit Dashboard
//!
//! ## Endpoints
//!
//! | Method    | Path                    | Description                              |
//! |-----------|-------------------------|------------------------------------------|
//! | GET (WS)  | `/ws/monitor`           | WebSocket real-time event stream         |
//! | GET       | `/api/monitor/position` | Open position ปัจจุบัน                    |
//! | GET       | `/api/monitor/history`  | Trade history ทั้งหมด                     |
//! | GET       | `/api/monitor/stats`    | tick_count, trade_count, uptime          |

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::atomic::Ordering;
use tracing::{debug, info};

use crate::{events::WsEvent, state::SharedState};

// ─── WebSocket Handler ────────────────────────────────────────────────────────

/// Upgrade HTTP → WebSocket แล้ว subscribe broadcast channel
///
/// SvelteKit ต่อที่ `ws://localhost:3000/ws/monitor`
/// ทุก WsEvent จะถูกส่งมาเป็น JSON text frame
pub async fn ws_monitor(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let mut rx = state.broadcast_tx.subscribe();
    let (mut sender, mut receiver) = socket.split();

    info!("🔌 WebSocket client connected");

    // ── ส่ง Snapshot ปัจจุบันทันทีที่ต่อ ─────────────────────────────────────
    let snapshot = {
        let strategy  = state.active_strategy.read().await.clone();
        let position  = state.open_position.read().await.clone();
        let ticks     = state.tick_count.load(Ordering::Relaxed);
        let trades    = state.trade_count.load(Ordering::Relaxed);

        json!({
            "event":        "SNAPSHOT",
            "strategy":     strategy,
            "position":     position,
            "tick_count":   ticks,
            "trade_count":  trades,
        })
        .to_string()
    };

    if sender.send(Message::Text(snapshot.into())).await.is_err() {
        return; // Client ปิดก่อน snapshot ส่งได้
    }

    // ── Event Loop ────────────────────────────────────────────────────────────
    loop {
        tokio::select! {
            // รับ Event จาก broadcast channel → ส่งต่อไป WebSocket client
            result = rx.recv() => {
                match result {
                    Ok(json_str) => {
                        if sender.send(Message::Text(json_str.into())).await.is_err() {
                            break; // Client disconnect
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // Client read ช้าเกินไป — บาง Event ถูก skip
                        debug!("WS client lagged, skipped {n} events");
                    }
                    Err(_) => break, // Channel closed
                }
            }

            // รับ Message จาก Client (Ping / Close)
            result = receiver.next() => {
                match result {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {} // Text/Binary from client — ignored for now
                }
            }
        }
    }

    info!("🔌 WebSocket client disconnected");
}

// ─── REST Monitoring Endpoints ────────────────────────────────────────────────

/// GET /api/monitor/position — ดู Position ที่เปิดอยู่
pub async fn get_position(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let position = state.open_position.read().await;
    Json(json!({
        "ok":       true,
        "position": *position,
    }))
}

/// GET /api/monitor/history — ดู Trade History ทั้งหมด
pub async fn get_history(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let history = state.trade_history.read().await;
    Json(json!({
        "ok":      true,
        "count":   history.len(),
        "records": *history,
    }))
}

/// GET /api/monitor/stats — สถิติ Server
pub async fn get_stats(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let tick_count   = state.tick_count.load(Ordering::Relaxed);
    let trade_count  = state.trade_count.load(Ordering::Relaxed);
    let has_strategy = state.active_strategy.read().await.is_some();
    let has_position = state.open_position.read().await.is_some();

    // Broadcast stats event ไปด้วยทุกครั้งที่มีคน poll
    state.broadcast(&WsEvent::ServerStats {
        tick_count,
        trade_count,
        has_position,
        has_strategy,
    });

    Json(json!({
        "ok":           true,
        "tick_count":   tick_count,
        "trade_count":  trade_count,
        "has_strategy": has_strategy,
        "has_position": has_position,
    }))
}
