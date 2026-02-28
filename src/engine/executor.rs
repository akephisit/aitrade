//! # engine::executor
//!
//! The **Trade Executor** — responsible for sending an execution command to
//! MetaTrader 5 once the Reflex Engine decides a trade should fire.
//!
//! This module is intentionally isolated from the evaluation logic so that:
//! * Unit tests can exercise `reflex::evaluate_tick` without needing a live MT5.
//! * The executor can be swapped for a paper-trading stub without touching
//!   any routing or evaluation code.

use tracing::info;

use crate::error::AppError;
use crate::models::{ActiveStrategy, Direction};

// ─── MT5 Order Request ────────────────────────────────────────────────────────

/// The payload we send to the MT5 HTTP adapter to open a position.
///
/// This mirrors the fields expected by a typical MT5 EA (Expert Advisor) order
/// endpoint.  Adjust field names to match your EA's API contract.
#[derive(Debug, serde::Serialize)]
pub struct Mt5OrderRequest {
    pub symbol: String,
    pub action: &'static str, // "BUY" | "SELL"
    pub volume: f64,
    pub price: f64,
    pub sl: f64,
    pub tp: f64,
    pub comment: String,
    pub magic: u64,           // EA "magic number" for identifying orders
}

// ─── Executor ─────────────────────────────────────────────────────────────────

/// Fire a trade execution command to MT5.
///
/// # Current Status
/// This is a **placeholder** implementation.  It logs the order that *would*
/// be sent and returns `Ok(())`.  Replace the body of this function with an
/// actual `reqwest` POST to your MT5 EA HTTP endpoint.
///
/// # Arguments
/// * `strategy`     — the strategy that triggered this trade.
/// * `entry_price`  — the exact price at which the trigger was hit.
/// * `mt5_base_url` — base URL of the MT5 HTTP adapter, e.g. `"http://localhost:8081"`.
pub async fn fire_trade(
    strategy: &ActiveStrategy,
    entry_price: f64,
    mt5_base_url: &str,
) -> Result<(), AppError> {
    let action = match strategy.direction {
        Direction::Buy => "BUY",
        Direction::Sell => "SELL",
        Direction::NoTrade => {
            return Err(AppError::BadRequest(
                "Cannot fire trade with NoTrade direction".into(),
            ))
        }
    };

    let order = Mt5OrderRequest {
        symbol:  strategy.symbol.clone(),
        action,
        volume:  strategy.lot_size,
        price:   entry_price,
        sl:      strategy.stop_loss,
        tp:      strategy.take_profit,
        comment: format!("AGV-{}", strategy.strategy_id),
        magic:   420001, // Unique magic number for Antigravity orders
    };

    info!(
        ?order,
        mt5_url = mt5_base_url,
        "🚀 [EXECUTOR] Sending trade order to MT5 (PLACEHOLDER — not sent)"
    );

    // ── TODO: Replace with real HTTP call ────────────────────────────────────
    //
    // let client = reqwest::Client::new();
    // let response = client
    //     .post(format!("{mt5_base_url}/order/send"))
    //     .json(&order)
    //     .send()
    //     .await
    //     .map_err(|e| AppError::ExecutionError(e.to_string()))?;
    //
    // if !response.status().is_success() {
    //     let body = response.text().await.unwrap_or_default();
    //     error!(body, "MT5 rejected the order");
    //     return Err(AppError::ExecutionError(body));
    // }
    //
    // info!("✅ MT5 order accepted");
    // ─────────────────────────────────────────────────────────────────────────

    Ok(())
}
