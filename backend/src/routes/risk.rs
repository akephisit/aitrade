//! # routes::risk
//!
//! Risk Management API Endpoints
//!
//! | Method | Path                    | Description               |
//! |--------|-------------------------|---------------------------|
//! | POST   | `/api/risk/kill`        | เปิด Kill Switch          |
//! | POST   | `/api/risk/rearm`       | ปิด Kill Switch           |
//! | GET    | `/api/risk/status`      | ดู Risk Status            |

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::state::SharedState;

#[derive(Deserialize)]
pub struct KillBody {
    pub reason: Option<String>,
}

/// POST /api/risk/kill — เปิด Kill Switch ฉุกเฉิน
pub async fn kill_switch_on(
    State(state): State<SharedState>,
    Json(body): Json<Option<KillBody>>,
) -> impl IntoResponse {
    let reason = body
        .and_then(|b| b.reason)
        .unwrap_or_else(|| "Manual kill via API".to_string());

    state.risk.kill(&reason).await;

    (StatusCode::OK, Json(json!({
        "ok":      true,
        "message": format!("Kill switch activated: {reason}"),
    })))
}

/// POST /api/risk/rearm — ปิด Kill Switch (re-enable trading)
pub async fn kill_switch_off(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    state.risk.rearm().await;

    Json(json!({
        "ok":      true,
        "message": "System re-armed — trading enabled",
    }))
}

/// GET /api/risk/status — ดู Risk Status ทั้งหมด
pub async fn get_risk_status(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let status = state.risk.status().await;
    Json(json!({ "ok": true, "risk": status }))
}
