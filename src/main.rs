//! # Antigravity — High-Performance Automated Trading Backend
//!
//! ## Architecture Overview
//!
//! ```text
//!  ┌──────────────┐   POST /api/brain/strategy   ┌──────────────────────┐
//!  │  OpenClaw    │ ─────────────────────────────▶│                      │
//!  │  (AI Agent)  │                               │   AppState           │
//!  └──────────────┘                               │   RwLock<Option<     │
//!                                                 │     ActiveStrategy>> │
//!  ┌──────────────┐   POST /api/mt5/tick          │                      │
//!  │  MetaTrader  │ ─────────────────────────────▶│   [Reflex Engine]    │──▶ fire_trade → MT5
//!  │  5 (EA)      │                               │                      │
//!  └──────────────┘                               └──────────────────────┘
//!                                                          │
//!  ┌──────────────┐   WebSocket / SSE                      │
//!  │  SvelteKit   │ ◀──────────────────────────────────────┘
//!  │  Frontend    │   GET /api/brain/strategy
//!  └──────────────┘   GET /api/mt5/health
//! ```
//!
//! ## Environment Variables
//!
//! | Variable        | Default           | Description                       |
//! |-----------------|-------------------|-----------------------------------|
//! | `BIND_ADDR`     | `0.0.0.0:3000`    | Address Axum listens on           |
//! | `MT5_BASE_URL`  | `http://localhost:8081` | Base URL of MT5 EA adapter  |
//! | `RUST_LOG`      | `antigravity=info`  | Tracing filter                  |

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
mod models;
mod routes;
mod state;

use routes::{
    brain::{clear_strategy, get_strategy, set_strategy},
    mt5::{handle_tick, health_check},
};
use state::build_state;

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Load .env (optional — CI/prod can use real env vars) ──────────────
    dotenvy::dotenv().ok();

    // ── 2. Initialise structured logging ─────────────────────────────────────
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env()
            .add_directive("antigravity=debug".parse()?)
            .add_directive("tower_http=info".parse()?))
        .init();

    info!(
        r#"

  ╔═══════════════════════════════════════════════╗
  ║        ANTIGRAVITY — Trading Backend          ║
  ║        Rust + Axum  ·  Brain & Reflex         ║
  ╚═══════════════════════════════════════════════╝"#
    );

    // ── 3. Build shared state ────────────────────────────────────────────────
    let state = build_state();

    // ── 4. Build CORS layer (allow SvelteKit dev server) ────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)   // Tighten in production!
        .allow_methods(Any)
        .allow_headers(Any);

    // ── 5. Build the Axum router ─────────────────────────────────────────────
    let app = Router::new()
        // ── Reflex Loop ──────────────────────────────────────────────────────
        .route("/api/mt5/tick",         post(handle_tick))
        .route("/api/mt5/health",       get(health_check))
        // ── Brain Loop ───────────────────────────────────────────────────────
        .route("/api/brain/strategy",   post(set_strategy))
        .route("/api/brain/strategy",   get(get_strategy))
        .route("/api/brain/strategy",   delete(clear_strategy))
        // ── Middleware ───────────────────────────────────────────────────────
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // ── 6. Resolve bind address ──────────────────────────────────────────────
    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()?;

    info!(?addr, "🚀 Antigravity server starting");

    // ── 7. Start the server ──────────────────────────────────────────────────
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
