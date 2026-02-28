//! # market — Market Data Snapshot
//!
//! ดึงข้อมูลตลาดมาสรุปให้ AI วิเคราะห์
//!
//! ## Data Sources (เลือกได้)
//! 1. MT5 Bridge API (mt5-bridge) — ถ้ามี MARKET_URL ใน .env
//! 2. Mock/Placeholder — สำหรับ dev/test โดยไม่ต้องมี MT5

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// Market Snapshot ที่รวบรวมแล้ว ส่งให้ AI วิเคราะห์
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol:         String,
    pub current_price:  f64,
    pub bid:            f64,
    pub ask:            f64,
    /// % เปลี่ยนแปลงใน 1 ชั่วโมงที่ผ่านมา
    pub change_1h_pct:  f64,
    /// % เปลี่ยนแปลงใน 24 ชั่วโมง
    pub change_24h_pct: f64,
    pub high_24h:       f64,
    pub low_24h:        f64,
    pub volume_24h:     f64,
    /// RSI 14 period (ถ้ามี)
    pub rsi_14:         Option<f64>,
    /// Moving average 20 period
    pub ma_20:          Option<f64>,
    /// Moving average 50 period
    pub ma_50:          Option<f64>,
}

/// Response format จาก mt5-bridge /api/market/snapshot
#[derive(Debug, Deserialize)]
struct BridgeMarketResponse {
    symbol:        String,
    bid:           f64,
    ask:           f64,
    change_1h:     Option<f64>,
    change_24h:    Option<f64>,
    high_24h:      Option<f64>,
    low_24h:       Option<f64>,
    volume:        Option<f64>,
    rsi_14:        Option<f64>,
    ma_20:         Option<f64>,
    ma_50:         Option<f64>,
}

/// ดึง Market Snapshot
/// ถ้ามี MARKET_URL → เรียก MT5 Bridge
/// ถ้าไม่มี → ใช้ Mock data (สำหรับ dev)
pub async fn fetch_market_snapshot(
    client: &reqwest::Client,
    config: &Config,
) -> anyhow::Result<MarketSnapshot> {
    if let Some(market_url) = &config.market_url {
        fetch_from_bridge(client, market_url, &config.symbol).await
    } else {
        tracing::warn!("MARKET_URL not set — using MOCK market data");
        Ok(mock_snapshot(&config.symbol))
    }
}

async fn fetch_from_bridge(
    client: &reqwest::Client,
    base_url: &str,
    symbol: &str,
) -> anyhow::Result<MarketSnapshot> {
    let url = format!("{base_url}/api/market/snapshot?symbol={symbol}");

    let resp: BridgeMarketResponse = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("Market API unreachable")?
        .json()
        .await
        .context("Failed to parse market response")?;

    let mid = (resp.bid + resp.ask) / 2.0;

    Ok(MarketSnapshot {
        symbol:         resp.symbol,
        current_price:  mid,
        bid:            resp.bid,
        ask:            resp.ask,
        change_1h_pct:  resp.change_1h.unwrap_or(0.0),
        change_24h_pct: resp.change_24h.unwrap_or(0.0),
        high_24h:       resp.high_24h.unwrap_or(mid * 1.01),
        low_24h:        resp.low_24h.unwrap_or(mid * 0.99),
        volume_24h:     resp.volume.unwrap_or(0.0),
        rsi_14:         resp.rsi_14,
        ma_20:          resp.ma_20,
        ma_50:          resp.ma_50,
    })
}

/// Mock data สำหรับ development (ไม่ต้องมี MT5)
fn mock_snapshot(symbol: &str) -> MarketSnapshot {
    MarketSnapshot {
        symbol:         symbol.to_string(),
        current_price:  67000.0,
        bid:            66998.0,
        ask:            67002.0,
        change_1h_pct:  0.15,
        change_24h_pct: 1.32,
        high_24h:       68500.0,
        low_24h:        65800.0,
        volume_24h:     24500.0,
        rsi_14:         Some(52.4),
        ma_20:          Some(66200.0),
        ma_50:          Some(64800.0),
    }
}
