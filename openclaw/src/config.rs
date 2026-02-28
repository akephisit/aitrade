//! # config — อ่าน Config จาก Environment Variables

use std::time::Duration;
use anyhow::{bail, Context};

/// AI Provider ที่รองรับ
#[derive(Debug, Clone)]
pub enum AiProvider {
    Claude,   // Anthropic Claude 3.5 Sonnet
    OpenAi,   // OpenAI GPT-4o
}

impl std::fmt::Display for AiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiProvider::Claude => write!(f, "Claude 3.5 Sonnet"),
            AiProvider::OpenAi => write!(f, "GPT-4o"),
        }
    }
}

/// Config ทั้งหมดที่ OpenClaw ต้องการ
#[derive(Debug, Clone)]
pub struct Config {
    /// "claude" หรือ "openai"
    pub ai_provider:      AiProvider,
    /// API Key สำหรับ Claude หรือ OpenAI
    pub api_key:          String,
    /// Symbol ที่จะวิเคราะห์ เช่น "BTCUSD"
    pub symbol:           String,
    /// URL ของ aitrade backend (Axum)
    pub aitrade_url:      String,
    /// รอบเวลา Brain Loop
    pub brain_interval:   Duration,
    /// Strategy มีอายุกี่นาที (หลังจากนี้ Reflex Loop จะไม่ใช้)
    pub strategy_ttl_min: u64,
    /// URL ดึงข้อมูลตลาด (MT5 Bridge หรือ Exchange API)
    pub market_url:       Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let provider_str = std::env::var("AI_PROVIDER")
            .unwrap_or_else(|_| "claude".to_string())
            .to_lowercase();

        let ai_provider = match provider_str.as_str() {
            "claude" => AiProvider::Claude,
            "openai" => AiProvider::OpenAi,
            other => bail!("Unknown AI_PROVIDER: '{other}'. Use 'claude' or 'openai'"),
        };

        let api_key = std::env::var("AI_API_KEY")
            .context("AI_API_KEY environment variable is required")?;

        let interval_secs: u64 = std::env::var("BRAIN_INTERVAL_SECS")
            .unwrap_or_else(|_| "300".to_string())  // default: 5 minutes
            .parse()
            .context("BRAIN_INTERVAL_SECS must be a number")?;

        Ok(Self {
            ai_provider,
            api_key,
            symbol:           std::env::var("SYMBOL").unwrap_or_else(|_| "BTCUSD".to_string()),
            aitrade_url:      std::env::var("AITRADE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            brain_interval:   Duration::from_secs(interval_secs),
            strategy_ttl_min: std::env::var("STRATEGY_TTL_MIN").unwrap_or_else(|_| "15".to_string()).parse().unwrap_or(15),
            market_url:       std::env::var("MARKET_URL").ok(),
        })
    }
}
