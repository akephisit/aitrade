//! # routes::brain
//!
//! Axum route handlers สำหรับ Brain Loop interface (OpenClaw → Axum)

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::AppError,
    events::WsEvent,
    models::ActiveStrategy,
    state::SharedState,
};

// ─── POST /api/brain/strategy ─────────────────────────────────────────────────

/// OpenClaw ส่งแผนใหม่มา — ติดตั้งใน State + Broadcast แจ้ง Dashboard
pub async fn set_strategy(
    State(state): State<SharedState>,
    Json(strategy): Json<ActiveStrategy>,
) -> Result<impl IntoResponse, AppError> {
    let id = strategy.strategy_id;

    // Broadcast ก่อน write เพื่อให้ Dashboard เห็นทันที
    state.broadcast(&WsEvent::StrategyUpdated {
        strategy: Box::new(strategy.clone()),
    });

    {
        let mut guard = state.active_strategy.write().await;
        *guard = Some(strategy);
    }

    tracing::info!(strategy_id = %id, "🧠 [BRAIN] New strategy installed");

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "ok":          true,
            "strategy_id": id,
            "message":     "Strategy activated — Reflex Loop is now armed.",
        })),
    ))
}

// ─── GET /api/brain/strategy ──────────────────────────────────────────────────

/// อ่าน Strategy ปัจจุบัน (SvelteKit ใช้ Poll นี้)
pub async fn get_strategy(
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, AppError> {
    let guard = state.active_strategy.read().await;

    match guard.as_ref() {
        Some(strategy) => Ok((
            StatusCode::OK,
            Json(json!({ "ok": true, "strategy": strategy })),
        )),
        None => Err(AppError::NotFound(
            "No active strategy. Brain Loop has not published a plan yet.".into(),
        )),
    }
}

// ─── DELETE /api/brain/strategy ───────────────────────────────────────────────

/// ล้าง Strategy — Disarm Reflex Loop ชั่วคราว
pub async fn clear_strategy(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    {
        let mut guard = state.active_strategy.write().await;
        *guard = None;
    }

    state.broadcast(&WsEvent::StrategyCleared);

    tracing::info!("🧠 [BRAIN] Strategy cleared — Reflex Loop disarmed");

    Json(json!({
        "ok":      true,
        "message": "Strategy cleared. Reflex Loop is now disarmed.",
    }))
}
