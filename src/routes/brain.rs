//! # routes::brain
//!
//! Axum route handlers for the **Brain Loop interface**.
//!
//! OpenClaw (or any client) calls these endpoints to install a new
//! `ActiveStrategy` into the shared state, making it immediately visible to
//! the Reflex Loop on the next tick.
//!
//! ## Endpoints
//!
//! | Method | Path                    | Description                                    |
//! |--------|-------------------------|------------------------------------------------|
//! | POST   | `/api/brain/strategy`   | Install / replace the current ActiveStrategy   |
//! | GET    | `/api/brain/strategy`   | Read the current ActiveStrategy (if any)       |
//! | DELETE | `/api/brain/strategy`   | Clear the current strategy (go flat / pause)   |

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::AppError,
    models::ActiveStrategy,
    state::SharedState,
};

// ─── POST /api/brain/strategy ─────────────────────────────────────────────────

/// Install (or replace) the `ActiveStrategy` in shared state.
///
/// OpenClaw calls this after completing its AI analysis.  The Reflex Loop will
/// start evaluating ticks against this new strategy on the very next tick that
/// arrives.
pub async fn set_strategy(
    State(state): State<SharedState>,
    Json(strategy): Json<ActiveStrategy>,
) -> Result<impl IntoResponse, AppError> {
    let id = strategy.strategy_id;

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

/// Return the currently active strategy (or a 404 if none exists).
///
/// The SvelteKit Monitor Loop uses this to display what the AI is currently
/// thinking.
pub async fn get_strategy(
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, AppError> {
    let guard = state.active_strategy.read().await;

    match guard.as_ref() {
        Some(strategy) => Ok((StatusCode::OK, Json(json!({ "ok": true, "strategy": strategy })))),
        None => Err(AppError::NotFound(
            "No active strategy. Brain Loop has not yet published a plan.".into(),
        )),
    }
}

// ─── DELETE /api/brain/strategy ───────────────────────────────────────────────

/// Clear the active strategy — disarms the Reflex Loop.
///
/// Call this when you want to pause trading without restarting the server.
pub async fn clear_strategy(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let mut guard = state.active_strategy.write().await;
    *guard = None;

    tracing::info!("🧠 [BRAIN] Strategy cleared — Reflex Loop disarmed");

    Json(json!({
        "ok":      true,
        "message": "Strategy cleared. Reflex Loop is now disarmed.",
    }))
}
