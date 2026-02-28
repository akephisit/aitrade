//! # Antigravity — High-Performance Automated Trading Backend
//!
//! ```text
//!  ┌──────────────┐  POST /api/brain/strategy   ┌──────────────────────────┐
//!  │  OpenClaw    │ ───────────────────────────▶ │  AppState                │
//!  │  (AI Agent)  │                              │  RwLock<ActiveStrategy>  │
//!  └──────────────┘                              │  RwLock<OpenPosition>    │
//!                                                │  RwLock<TradeHistory>    │
//!  ┌──────────────┐  POST /api/mt5/tick          │  broadcast_tx ──────────┐│
//!  │  MetaTrader  │ ───────────────────────────▶ │  [Reflex Engine]         ││
//!  │  5 (EA)      │  ← POST /order/send (MT5)    │  [Executor]              ││
//!  └──────────────┘                              └──────────────────────────┘│
//!                                                         │                  │
//!  ┌──────────────┐  ws://..../ws/monitor  ◀─────────────┘◀─────────────────┘
//!  │  SvelteKit   │  GET /api/monitor/position
//!  │  Dashboard   │  GET /api/monitor/history
//!  └──────────────┘  GET /api/monitor/stats
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

mod engine;
mod error;
mod events;
mod models;
mod routes;
mod state;

use routes::{
    brain::{clear_strategy, get_strategy, set_strategy},
    monitor::{get_history, get_position, get_stats, ws_monitor},
    mt5::{handle_tick, health_check},
};
use state::build_state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Load .env ──────────────────────────────────────────────────────────
    dotenvy::dotenv().ok();

    // ── 2. Structured logging ──────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::from_default_env()
                .add_directive("antigravity=debug".parse()?)
                .add_directive("tower_http=info".parse()?),
        )
        .init();

    info!(
        r#"

  ╔═══════════════════════════════════════════════════╗
  ║          ANTIGRAVITY — Trading Backend            ║
  ║   Brain & Reflex · Position Mgmt · WS Monitor    ║
  ╚═══════════════════════════════════════════════════╝"#
    );

    // ── 3. Shared state ────────────────────────────────────────────────────────
    let state = build_state();

    // ── 4. CORS (ให้ SvelteKit dev server ต่อได้) ──────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)   // ปรับให้ strict ใน production
        .allow_methods(Any)
        .allow_headers(Any);

    // ── 5. Router ──────────────────────────────────────────────────────────────
    let app = Router::new()
        // ── Reflex Loop (MT5 → Axum) ──────────────────────────────────────────
        .route("/api/mt5/tick",           post(handle_tick))
        .route("/api/mt5/health",         get(health_check))
        // ── Brain Loop (OpenClaw → Axum) ──────────────────────────────────────
        .route("/api/brain/strategy",     post(set_strategy))
        .route("/api/brain/strategy",     get(get_strategy))
        .route("/api/brain/strategy",     delete(clear_strategy))
        // ── Monitor Loop (SvelteKit → Axum) ───────────────────────────────────
        .route("/ws/monitor",             get(ws_monitor))       // WebSocket
        .route("/api/monitor/position",   get(get_position))     // REST
        .route("/api/monitor/history",    get(get_history))
        .route("/api/monitor/stats",      get(get_stats))
        // ── Middleware ─────────────────────────────────────────────────────────
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // ── 6. Bind & Serve ────────────────────────────────────────────────────────
    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()?;

    info!(?addr, "🚀 Antigravity server starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
