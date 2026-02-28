//! # engine::executor
//!
//! **Trade Executor** — ยิง Order จริงไปที่ MT5 ผ่าน HTTP
//!
//! ## MT5 EA API Contract (ฝั่ง MQL5)
//! EA ต้องรับ POST `/order/send` และคืน JSON:
//! ```json
//! { "retcode": 10009, "order": 123456, "comment": "Request completed" }
//! ```
//! retcode 10009 = `TRADE_RETCODE_DONE` (สำเร็จ)

use tracing::{error, info, warn};

use crate::error::AppError;
use crate::models::Direction;

// ─── MT5 Request / Response ───────────────────────────────────────────────────

/// Payload ที่ส่งไปยัง MT5 EA endpoint
#[derive(Debug, serde::Serialize)]
pub struct Mt5OrderRequest {
    pub symbol:  String,
    pub action:  &'static str,  // "BUY" | "SELL"
    pub volume:  f64,
    pub price:   f64,
    pub sl:      f64,
    pub tp:      f64,
    pub comment: String,
    pub magic:   u64,           // Antigravity magic number
}

/// Response จาก MT5 EA
#[derive(Debug, serde::Deserialize)]
pub struct Mt5OrderResponse {
    /// MT5 Return Code — 10009 = SUCCESS
    pub retcode: u32,
    /// MT5 Ticket / Order ID (มีเมื่อ retcode = 10009)
    pub order:   Option<u64>,
    /// ข้อความอธิบายจาก MT5
    pub comment: Option<String>,
}

// ─── Build Order ──────────────────────────────────────────────────────────────

/// สร้าง `Mt5OrderRequest` จาก Strategy + entry price
pub fn build_order(
    symbol: &str,
    direction: Direction,
    entry_price: f64,
    sl: f64,
    tp: f64,
    lot_size: f64,
    strategy_id: uuid::Uuid,
) -> Result<Mt5OrderRequest, AppError> {
    let action = match direction {
        Direction::Buy  => "BUY",
        Direction::Sell => "SELL",
        Direction::NoTrade => {
            return Err(AppError::BadRequest(
                "Cannot build order for NoTrade direction".into(),
            ))
        }
    };

    Ok(Mt5OrderRequest {
        symbol:  symbol.to_string(),
        action,
        volume:  lot_size,
        price:   entry_price,
        sl,
        tp,
        comment: format!("AGV-{}", &strategy_id.to_string()[..8]),
        magic:   420001,
    })
}

// ─── Fire Trade ───────────────────────────────────────────────────────────────

/// ส่ง Order ไปที่ MT5 EA และรอ Response
///
/// คืน `Mt5OrderResponse` ถ้าสำเร็จ, `AppError::ExecutionError` ถ้าล้มเหลว
pub async fn fire_trade(
    order: &Mt5OrderRequest,
    client: &reqwest::Client,
    mt5_base_url: &str,
) -> Result<Mt5OrderResponse, AppError> {
    if mt5_base_url == "mock" {
        info!("🎭 [EXECUTOR] Running in MOCK mode — simulating MT5 success");
        return Ok(Mt5OrderResponse {
            retcode: 10009,
            order:   Some(999999),
            comment: Some("Mock Order".to_string()),
        });
    }

    let url = format!("{mt5_base_url}/order/send");

    info!(
        symbol    = %order.symbol,
        action    = %order.action,
        volume    = order.volume,
        price     = order.price,
        sl        = order.sl,
        tp        = order.tp,
        mt5_url   = %url,
        "🚀 [EXECUTOR] Sending order to MT5"
    );

    // ── HTTP POST ─────────────────────────────────────────────────────────────
    let response = client
        .post(&url)
        .json(order)
        .timeout(std::time::Duration::from_secs(5))   // ห้ามรอนานกว่า 5 วิ
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "MT5 unreachable");
            AppError::ExecutionError(format!("MT5 unreachable: {e}"))
        })?;

    // ── HTTP Status ───────────────────────────────────────────────────────────
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(http_status = %status, body = %body, "MT5 returned HTTP error");
        return Err(AppError::ExecutionError(
            format!("MT5 HTTP {status}: {body}")
        ));
    }

    // ── Parse Response ────────────────────────────────────────────────────────
    let mt5_resp: Mt5OrderResponse = response
        .json()
        .await
        .map_err(|e| {
            error!(error = %e, "MT5 response parse failed");
            AppError::ExecutionError(format!("MT5 response parse error: {e}"))
        })?;

    // ── Check retcode ─────────────────────────────────────────────────────────
    // 10009 = TRADE_RETCODE_DONE (เท่านั้นที่ถือว่า success)
    if mt5_resp.retcode != 10009 {
        let msg = format!(
            "MT5 rejected: retcode={} comment={}",
            mt5_resp.retcode,
            mt5_resp.comment.as_deref().unwrap_or("unknown")
        );
        warn!("{msg}");
        return Err(AppError::ExecutionError(msg));
    }

    info!(
        ticket = ?mt5_resp.order,
        "✅ [EXECUTOR] MT5 accepted order"
    );

    Ok(mt5_resp)
}
