//! # strategy — Parse AI Response เป็น ActiveStrategy
//!
//! AI คืน JSON ตรงๆ → parse เป็น struct → กำหนด metadata

use anyhow::Context;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;

/// Struct ที่ตรงกับ JSON schema ที่ Prompt กำหนด
#[derive(Debug, Deserialize)]
struct AiStrategyJson {
    direction:        String,    // "BUY" | "SELL" | "NO_TRADE"
    entry_zone_low:   f64,
    entry_zone_high:  f64,
    take_profit:      f64,
    stop_loss:        f64,
    lot_size:         f64,
    rationale:        String,
}

/// ActiveStrategy ที่จะ POST ไปยัง aitrade
/// (ต้อง match กับ struct ใน aitrade/src/models/strategy.rs)
#[derive(Debug, serde::Serialize)]
pub struct ActiveStrategy {
    pub strategy_id:  Uuid,
    pub symbol:       String,
    pub direction:    Direction,
    pub entry_zone:   EntryZone,
    pub take_profit:  f64,
    pub stop_loss:    f64,
    pub lot_size:     f64,
    pub rationale:    String,
    pub created_at:   chrono::DateTime<Utc>,
    pub expires_at:   Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    Buy,
    Sell,
    NoTrade,
}

#[derive(Debug, serde::Serialize)]
pub struct EntryZone {
    pub low:  f64,
    pub high: f64,
}

/// แปลง AI response text เป็น ActiveStrategy
pub fn parse_strategy_from_ai(
    ai_text: &str,
    symbol: &str,
    config: &Config,
) -> anyhow::Result<ActiveStrategy> {
    // Strip markdown code fences ถ้า AI ใส่มา
    let cleaned = strip_markdown(ai_text);

    let parsed: AiStrategyJson = serde_json::from_str(&cleaned)
        .with_context(|| format!("AI returned invalid JSON: {cleaned}"))?;

    let direction = match parsed.direction.to_uppercase().as_str() {
        "BUY"      => Direction::Buy,
        "SELL"     => Direction::Sell,
        "NO_TRADE" => Direction::NoTrade,
        other => anyhow::bail!("Unknown direction from AI: '{other}'"),
    };

    let now = Utc::now();
    let expires_at = Some(now + chrono::Duration::minutes(config.strategy_ttl_min as i64));

    Ok(ActiveStrategy {
        strategy_id: Uuid::new_v4(),
        symbol:      symbol.to_string(),
        direction,
        entry_zone:  EntryZone {
            low:  parsed.entry_zone_low,
            high: parsed.entry_zone_high,
        },
        take_profit: parsed.take_profit,
        stop_loss:   parsed.stop_loss,
        lot_size:    parsed.lot_size,
        rationale:   parsed.rationale,
        created_at:  now,
        expires_at,
    })
}

/// ลบ markdown code fences ที่ AI อาจใส่มา
fn strip_markdown(text: &str) -> String {
    let text = text.trim();
    // ลบ ```json ... ``` หรือ ``` ... ```
    if let Some(inner) = text.strip_prefix("```json") {
        inner.trim_end_matches("```").trim().to_string()
    } else if let Some(inner) = text.strip_prefix("```") {
        inner.trim_end_matches("```").trim().to_string()
    } else {
        text.to_string()
    }
}
