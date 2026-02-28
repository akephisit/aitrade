//! # auth — API Key Middleware
//!
//! ป้องกัน Endpoint ด้วย `X-API-Key` header
//!
//! ## Mode
//! - `API_KEY` ไม่ได้ตั้ง (หรือ empty) → **Allow All** (Dev Mode)
//! - `API_KEY` ตั้งค่า → ต้องส่ง `X-API-Key: <key>` ทุก Request
//!
//! ## ยกเว้น
//! Health check endpoints ไม่ต้อง Auth (/api/mt5/health)
//!
//! ## Usage
//! ```bash
//! API_KEY=super-secret-key-here cargo run
//! ```
//! ```bash
//! curl -H "X-API-Key: super-secret-key-here" http://localhost:3000/api/brain/strategy
//! ```

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use tracing::warn;

/// Axum middleware — ตรวจสอบ X-API-Key header
///
/// ถ้า `API_KEY` env ว่างหรือไม่ได้ตั้ง → pass through ทันที (dev mode)
pub async fn require_api_key(request: Request<Body>, next: Next) -> Response {
    let api_key_env = std::env::var("API_KEY").unwrap_or_default();

    // ── Dev Mode: ไม่มี API_KEY → ยอมให้ผ่านหมด ─────────────────────────────
    if api_key_env.is_empty() {
        return next.run(request).await;
    }

    // ── ยกเว้น Health Check ───────────────────────────────────────────────────
    let path = request.uri().path();
    if path == "/api/mt5/health" || path == "/health" {
        return next.run(request).await;
    }

    // ── ตรวจสอบ Header ────────────────────────────────────────────────────────
    let provided = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided == api_key_env {
        next.run(request).await
    } else {
        warn!(path, "❌ Unauthorized request — invalid or missing X-API-Key");
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok":    false,
                "error": "Unauthorized: invalid or missing X-API-Key header",
                "hint":  "Set X-API-Key header with your API key"
            })),
        )
            .into_response()
    }
}
