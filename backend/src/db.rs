//! # db — PostgreSQL Database Layer
//!
//! ใช้ `sqlx` สำหรับ async PostgreSQL — compile-time checked queries
//!
//! ## Setup
//! 1. ติดตั้ง PostgreSQL และสร้าง database
//! 2. ตั้ง `DATABASE_URL` ใน `.env`
//! 3. รัน migration: `psql $DATABASE_URL -f migrations/001_init.sql`

use anyhow::Context;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::info;

use crate::models::{position::TradeRecord, ActiveStrategy};

// ─── Pool Init ────────────────────────────────────────────────────────────────

/// สร้าง PgPool และ run migration
pub async fn init_pool(database_url: &str) -> anyhow::Result<PgPool> {
    info!("Connecting to PostgreSQL...");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    // Run migrations
    run_migrations(&pool).await?;

    info!("✅ PostgreSQL connected and migrations applied");
    Ok(pool)
}

async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    // Embedded migration SQL
    sqlx::query(include_str!("../migrations/001_init.sql"))
        .execute(pool)
        .await
        .context("Failed to run migration 001_init.sql")?;

    Ok(())
}

// ─── Trade Records ────────────────────────────────────────────────────────────

/// บันทึก TradeRecord ลง PostgreSQL
pub async fn insert_trade_record(
    pool:   &PgPool,
    record: &TradeRecord,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO trade_records
          (trade_id, strategy_id, symbol, direction, entry_price,
           lot_size, take_profit, stop_loss, mt5_ticket, status, status_message, fired_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (trade_id) DO UPDATE SET
          status         = EXCLUDED.status,
          status_message = EXCLUDED.status_message,
          mt5_ticket     = EXCLUDED.mt5_ticket
        "#,
        record.trade_id,
        record.strategy_id,
        record.symbol,
        format!("{:?}", record.direction),
        record.entry_price,
        record.lot_size,
        record.take_profit,
        record.stop_loss,
        record.mt5_ticket.map(|t| t as i64),
        format!("{:?}", record.status),
        &record.status_message,
        record.fired_at,
    )
    .execute(pool)
    .await
    .context("insert_trade_record failed")?;

    Ok(())
}

/// โหลด Trade History ทั้งหมดเพื่อ seed in-memory state ตอน startup
pub async fn load_trade_history(pool: &PgPool) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = sqlx::query_as!(
        TradeRow,
        r#"
        SELECT trade_id, strategy_id, symbol, direction, entry_price,
               lot_size, take_profit, stop_loss, mt5_ticket, status,
               status_message, fired_at
        FROM trade_records
        ORDER BY fired_at DESC
        LIMIT 500
        "#
    )
    .fetch_all(pool)
    .await
    .context("load_trade_history failed")?;

    Ok(rows.into_iter().map(|r| serde_json::json!(r)).collect())
}

#[derive(sqlx::FromRow, serde::Serialize)]
struct TradeRow {
    trade_id:       uuid::Uuid,
    strategy_id:    uuid::Uuid,
    symbol:         String,
    direction:      String,
    entry_price:    sqlx::types::BigDecimal,
    lot_size:       sqlx::types::BigDecimal,
    take_profit:    sqlx::types::BigDecimal,
    stop_loss:      sqlx::types::BigDecimal,
    mt5_ticket:     Option<i64>,
    status:         String,
    status_message: Option<String>,
    fired_at:       chrono::DateTime<chrono::Utc>,
}

// ─── Strategy Log ─────────────────────────────────────────────────────────────

/// บันทึกทุก Strategy ที่ OpenClaw ส่งมา (สำหรับ analysis)
pub async fn log_strategy(
    pool:     &PgPool,
    strategy: &ActiveStrategy,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO strategy_log
          (strategy_id, symbol, direction, entry_zone_low, entry_zone_high,
           take_profit, stop_loss, lot_size, rationale, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (strategy_id) DO NOTHING
        "#,
        strategy.strategy_id,
        strategy.symbol,
        format!("{:?}", strategy.direction),
        strategy.entry_zone.low,
        strategy.entry_zone.high,
        strategy.take_profit,
        strategy.stop_loss,
        strategy.lot_size,
        &strategy.rationale,
        strategy.created_at,
        strategy.expires_at,
    )
    .execute(pool)
    .await
    .context("log_strategy failed")?;

    Ok(())
}
