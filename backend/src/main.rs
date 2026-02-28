//! # Antigravity — High-Performance Automated Trading Backend
//!
//! ```text
//!  ┌─────────────┐  POST /api/brain/strategy  ┌─────────────────────────────┐
//!  │  OpenClaw   │ ─────────────────────────▶ │ AppState                    │
//!  │  (AI Agent) │                             │ ├─ active_strategy          │
//!  └─────────────┘                             │ ├─ open_position            │
//!                                              │ ├─ trade_history            │
//!  ┌─────────────┐  POST /api/mt5/tick         │ ├─ risk_manager  🛡️         │
//!  │  MT5 EA     │ ─────────────────────────▶ │ ├─ tick_buffer              │
//!  └─────────────┘  ← POST /order/send         │ └─ broadcast_tx ──────────┐ │
//!                                              └────────────────────────────┘ │
//!  ┌─────────────┐  ws://host/ws/monitor  ◀────────────────────────────────── ┘
//!  │  Dashboard  │  GET  /api/monitor/*
//!  └─────────────┘  POST /api/backtest   📊
//!                   POST /api/risk/kill  ⛔
//! ```

use std::net::SocketAddr;

use axum::{
    Router,
    routing::{delete, get, post},
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod auth;
mod engine;
mod error;
mod events;
mod models;
mod risk;
mod routes;
mod state;

use auth::require_api_key;
use routes::{
    backtest::run_backtest,
    brain::{clear_strategy, get_strategy, set_strategy},
    monitor::{get_history, get_position, get_stats, ws_monitor},
    mt5::{handle_position_close, handle_tick, health_check},
    risk::{get_risk_status, kill_switch_off, kill_switch_on},
};
use state::build_state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Load .env ──────────────────────────────────────────────────────────
    dotenvy::dotenv().ok();

    // ── 2. Structured logging ─────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::from_default_env()
                .add_directive("antigravity=debug".parse()?)
                .add_directive("tower_http=info".parse()?),
        )
        .init();

    info!(r#"

  ╔═══════════════════════════════════════════════════════╗
  ║           ANTIGRAVITY — Trading Backend               ║
  ║  Brain · Reflex · Confirmation · Risk · Backtest      ║
  ╚═══════════════════════════════════════════════════════╝"#);

    // ── 3. Shared state ───────────────────────────────────────────────────────
    let state = build_state();

    // ── 4. CORS ───────────────────────────────────────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // ── 5. Router ─────────────────────────────────────────────────────────────
    let app = Router::new()
        // ── Reflex Loop ───────────────────────────────────────────────────────
        .route("/api/mt5/tick",           post(handle_tick))
        .route("/api/mt5/health",         get(health_check))
        .route("/api/mt5/position-close", post(handle_position_close))
        // ── Brain Loop ────────────────────────────────────────────────────────
        .route("/api/brain/strategy",     post(set_strategy))
        .route("/api/brain/strategy",     get(get_strategy))
        .route("/api/brain/strategy",     delete(clear_strategy))
        // ── Monitor Loop ──────────────────────────────────────────────────────
        .route("/ws/monitor",             get(ws_monitor))
        .route("/api/monitor/position",   get(get_position))
        .route("/api/monitor/history",    get(get_history))
        .route("/api/monitor/stats",      get(get_stats))
        // ── Risk Management ───────────────────────────────────────────────────
        .route("/api/risk/kill",          post(kill_switch_on))
        .route("/api/risk/rearm",         post(kill_switch_off))
        .route("/api/risk/status",        get(get_risk_status))
        // ── Backtesting ───────────────────────────────────────────────────────
        .route("/api/backtest",           post(run_backtest))
        // ── Middleware ────────────────────────────────────────────────────────
        .layer(axum::middleware::from_fn(require_api_key))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // ── 6. Bind & Serve ───────────────────────────────────────────────────────
    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()?;

    info!(?addr, "🚀 Antigravity server starting");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
