//! # OpenClaw — AI Brain Agent
//!
//! Brain Loop ที่วิ่งอิสระ แยกจาก Backend โดยสิ้นเชิง
//!
//! ## Flow
//! ```text
//! loop every N minutes:
//!   1. Fetch market snapshot (OHLCV + indicators)
//!   2. Build prompt สำหรับ AI
//!   3. Call Claude 3.5 / GPT-4o API
//!   4. Parse JSON response → ActiveStrategy
//!   5. POST → aitrade /api/brain/strategy
//! ```

use anyhow::Context;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod ai;
mod config;
mod market;
mod poster;
mod prompt;
mod strategy;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env()
            .add_directive("openclaw=debug".parse()?)
            .add_directive("reqwest=warn".parse()?))
        .init();

    info!(r#"

  ╔═══════════════════════════════════════════╗
  ║   OPENCLAW — AI Brain Agent               ║
  ║   Antigravity Trading System              ║
  ╚═══════════════════════════════════════════╝"#);

    let config = Config::from_env().context("Failed to load config")?;
    let client = reqwest::Client::new();

    info!(
        symbol   = %config.symbol,
        provider = %config.ai_provider,
        interval = ?config.brain_interval,
        backend  = %config.aitrade_url,
        "OpenClaw started"
    );

    // ── Brain Loop ────────────────────────────────────────────────────────────
    loop {
        info!("🧠 Brain Loop cycle starting...");

        match run_cycle(&config, &client).await {
            Ok(strategy_id) => {
                info!(strategy_id = %strategy_id, "✅ Strategy posted successfully");
            }
            Err(e) => {
                error!(error = %e, "❌ Brain cycle failed — will retry next interval");
            }
        }

        info!(
            interval = ?config.brain_interval,
            "💤 Sleeping until next cycle..."
        );
        tokio::time::sleep(config.brain_interval).await;
    }
}

/// ทำ 1 รอบของ Brain Loop:
/// fetch → build prompt → call AI → parse → POST
async fn run_cycle(
    config: &Config,
    client: &reqwest::Client,
) -> anyhow::Result<uuid::Uuid> {
    // 1. ดึงข้อมูลตลาด
    let snapshot = market::fetch_market_snapshot(client, config).await
        .context("Failed to fetch market snapshot")?;

    info!(
        symbol    = %snapshot.symbol,
        price     = snapshot.current_price,
        change_1h = snapshot.change_1h_pct,
        "Market snapshot fetched"
    );

    // 2. สร้าง Prompt
    let prompt = prompt::build_prompt(&snapshot, config);

    // 3. เรียก AI
    let ai_response = ai::call_ai(client, config, &prompt).await
        .context("AI API call failed")?;

    info!("AI response received ({} chars)", ai_response.len());

    // 4. Parse เป็น Strategy
    let strategy = strategy::parse_strategy_from_ai(&ai_response, &snapshot.symbol, config)
        .context("Failed to parse AI response into strategy")?;

    let strategy_id = strategy.strategy_id;

    // 5. POST ไป aitrade
    poster::post_strategy(client, config, &strategy).await
        .context("Failed to POST strategy to aitrade")?;

    Ok(strategy_id)
}
