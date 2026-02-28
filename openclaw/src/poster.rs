//! # poster — POST ActiveStrategy ไปยัง aitrade Backend

use anyhow::Context;
use tracing::info;

use crate::{config::Config, strategy::ActiveStrategy};

/// POST strategy ไปที่ aitrade /api/brain/strategy
pub async fn post_strategy(
    client: &reqwest::Client,
    config: &Config,
    strategy: &ActiveStrategy,
) -> anyhow::Result<()> {
    let url = format!("{}/api/brain/strategy", config.aitrade_url);

    info!(
        strategy_id = %strategy.strategy_id,
        direction   = ?strategy.direction,
        url         = %url,
        "Posting strategy to aitrade..."
    );

    let resp = client
        .post(&url)
        .json(strategy)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("aitrade backend unreachable")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("aitrade rejected strategy: HTTP {status}: {body}");
    }

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    info!(response = %body, "Strategy accepted by aitrade ✅");

    Ok(())
}
