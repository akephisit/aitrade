//! # routes::mt5
//!
//! Axum route handlers for the **MetaTrader 5 interface**.
//!
//! ## Endpoints
//!
//! | Method | Path                    | Description                                      |
//! |--------|-------------------------|--------------------------------------------------|
//! | POST   | `/api/mt5/tick`         | Receive a price tick, run Reflex evaluation      |
//! | GET    | `/api/mt5/health`       | MT5 adapter connectivity check                   |

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::error;

use crate::{
    engine::{
        executor::fire_trade,
        reflex::{evaluate_tick, TradeSignal},
    },
    error::AppError,
    models::TickData,
    state::SharedState,
};

// ─── POST /api/mt5/tick ───────────────────────────────────────────────────────

/// **Reflex Loop entry point.**
///
/// MT5 calls this endpoint on every price update (or every N milliseconds for
/// high-frequency instruments).  The handler must return quickly — it delegates
/// all heavy logic to the engine layer.
///
/// ### Request body (JSON)
/// ```json
/// {
///   "symbol": "BTCUSD",
///   "bid": 67010.50,
///   "ask": 67012.00,
///   "volume": 1.5,
///   "time": "2025-01-01T12:00:00Z"
/// }
/// ```
///
/// ### Response
/// * `200 OK` with `{ "ok": true, "action": "NO_ACTION" | "TRADE_TRIGGERED" }`
/// * `4xx / 5xx` on errors (see [`AppError`])
pub async fn handle_tick(
    State(state): State<SharedState>,
    Json(tick): Json<TickData>,
) -> Result<impl IntoResponse, AppError> {
    // ── Step 1: Run the Reflex Engine evaluation ──────────────────────────────
    let signal = evaluate_tick(&tick, &state).await?;

    // ── Step 2: Act on the signal ─────────────────────────────────────────────
    match signal {
        TradeSignal::NoAction => {
            // Fast path — vast majority of ticks land here.
            Ok((
                StatusCode::OK,
                Json(json!({
                    "ok":     true,
                    "action": "NO_ACTION",
                    "symbol": tick.symbol,
                    "bid":    tick.bid,
                    "ask":    tick.ask,
                })),
            ))
        }

        TradeSignal::Trigger(strategy) => {
            // ── Step 2a: Determine the exact fill price ───────────────────────
            let entry_price = match strategy.direction {
                crate::models::Direction::Buy  => tick.ask,
                crate::models::Direction::Sell => tick.bid,
                crate::models::Direction::NoTrade => tick.effective_mid(),
            };

            // ── Step 2b: Delegate to the executor ────────────────────────────
            // We intentionally do NOT await a response here in the hot path if
            // you need sub-millisecond latency — use `tokio::spawn` to fire and
            // forget.  For now we await for simplicity and observability.
            let mt5_url = std::env::var("MT5_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8081".to_string());

            if let Err(e) = fire_trade(&strategy, entry_price, &mt5_url).await {
                error!(error = %e, "Trade execution failed");
                return Err(e);
            }

            // ── Step 2c: After firing, clear the strategy so we don't
            //    double-trigger on the next tick (one trade per signal). ───────
            {
                let mut guard = state.active_strategy.write().await;
                *guard = None;
            }

            Ok((
                StatusCode::OK,
                Json(json!({
                    "ok":          true,
                    "action":      "TRADE_TRIGGERED",
                    "strategy_id": strategy.strategy_id,
                    "symbol":      strategy.symbol,
                    "direction":   strategy.direction,
                    "entry_price": entry_price,
                    "tp":          strategy.take_profit,
                    "sl":          strategy.stop_loss,
                })),
            ))
        }
    }
}

// ─── GET /api/mt5/health ──────────────────────────────────────────────────────

/// Simple health-check for the MT5 adapter.
/// The SvelteKit Monitor Loop polls this to display MT5 connectivity status.
pub async fn health_check(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let tick_count  = state.tick_count.load(std::sync::atomic::Ordering::Relaxed);
    let trade_count = state.trade_count.load(std::sync::atomic::Ordering::Relaxed);
    let has_strategy = state.active_strategy.read().await.is_some();

    Json(json!({
        "ok":           true,
        "tick_count":   tick_count,
        "trade_count":  trade_count,
        "has_strategy": has_strategy,
    }))
}
