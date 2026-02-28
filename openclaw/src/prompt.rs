//! # prompt — สร้าง Prompt สำหรับ AI
//!
//! Prompt ที่ดีคือหัวใจของ Brain Loop
//! AI ต้องคืน JSON ที่ parse เป็น ActiveStrategy ได้ทันที

use crate::{config::Config, market::MarketSnapshot};

/// สร้าง Prompt ที่บังคับให้ AI ตอบเป็น JSON ที่ parse ได้
pub fn build_prompt(snapshot: &MarketSnapshot, config: &Config) -> String {
    let rsi_line = snapshot.rsi_14
        .map(|v| format!("- RSI(14): {v:.1}"))
        .unwrap_or_else(|| "- RSI(14): N/A".to_string());

    let ma_line = match (snapshot.ma_20, snapshot.ma_50) {
        (Some(m20), Some(m50)) => format!("- MA20: {m20:.2} | MA50: {m50:.2} | Price vs MA20: {:.2}%",
            ((snapshot.current_price - m20) / m20) * 100.0),
        _ => "- MA: N/A".to_string(),
    };

    let ttl = config.strategy_ttl_min;
    let symbol = &snapshot.symbol;

    format!(r#"You are an expert algorithmic trader analyzing {symbol}.

## Current Market Data
- Symbol: {symbol}
- Current Price: {price:.2}
- Bid/Ask: {bid:.2} / {ask:.2}
- 1H Change: {ch1h:+.2}%
- 24H Change: {ch24h:+.2}%
- 24H High: {high:.2} | 24H Low: {low:.2}
{rsi_line}
{ma_line}

## Your Task
Analyze the market conditions and provide a precise trading strategy.

**CRITICAL**: Respond with ONLY a valid JSON object. No explanations, no markdown, no code fences.

## Required JSON Format
```
{{
  "direction": "BUY" | "SELL" | "NO_TRADE",
  "entry_zone_low": <float>,
  "entry_zone_high": <float>,
  "take_profit": <float>,
  "stop_loss": <float>,
  "lot_size": 0.10,
  "rationale": "<brief explanation max 100 chars>"
}}
```

## Rules
1. entry_zone_low < entry_zone_high always
2. For BUY: stop_loss < entry_zone_low < entry_zone_high < take_profit
3. For SELL: take_profit < entry_zone_low < entry_zone_high < stop_loss
4. If market conditions are unclear → use "NO_TRADE"
5. Entry zone width should be 20-100 pips max
6. Risk/Reward ratio must be >= 1.5
7. Strategy is valid for {ttl} minutes

Respond with JSON only:"#,
        price  = snapshot.current_price,
        bid    = snapshot.bid,
        ask    = snapshot.ask,
        ch1h   = snapshot.change_1h_pct,
        ch24h  = snapshot.change_24h_pct,
        high   = snapshot.high_24h,
        low    = snapshot.low_24h,
    )
}
