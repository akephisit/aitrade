//! # ai — เรียก Claude หรือ OpenAI API
//!
//! รองรับทั้ง 2 provider ผ่าน `AI_PROVIDER` env var

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config::{AiProvider, Config};

// ─── Shared Response ──────────────────────────────────────────────────────────

/// เรียก AI ตาม provider ที่ config กำหนด
/// คืน raw text response จาก AI (JSON string ที่ยังไม่ parse)
pub async fn call_ai(
    client: &reqwest::Client,
    config: &Config,
    prompt: &str,
) -> anyhow::Result<String> {
    match config.ai_provider {
        AiProvider::Claude => call_claude(client, config, prompt).await,
        AiProvider::OpenAi => call_openai(client, config, prompt).await,
    }
}

// ─── Anthropic Claude ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ClaudeRequest<'a> {
    model:      &'a str,
    max_tokens: u32,
    messages:   Vec<ClaudeMessage<'a>>,
}

#[derive(Serialize)]
struct ClaudeMessage<'a> {
    role:    &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

async fn call_claude(
    client: &reqwest::Client,
    config: &Config,
    prompt: &str,
) -> anyhow::Result<String> {
    let body = ClaudeRequest {
        model:      "claude-3-5-sonnet-20241022",
        max_tokens: 512,
        messages:   vec![ClaudeMessage { role: "user", content: prompt }],
    };

    debug!("Calling Claude API...");

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("Claude API request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Claude API error {status}: {text}");
    }

    let data: ClaudeResponse = resp.json().await.context("Claude response parse error")?;

    data.content.into_iter().next()
        .map(|c| c.text)
        .context("Claude returned empty content")
}

// ─── OpenAI GPT-4o ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model:    &'a str,
    messages: Vec<OpenAiMessage<'a>>,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role:    &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMsg,
}

#[derive(Deserialize)]
struct OpenAiChoiceMsg {
    content: Option<String>,
}

async fn call_openai(
    client: &reqwest::Client,
    config: &Config,
    prompt: &str,
) -> anyhow::Result<String> {
    let body = OpenAiRequest {
        model:    "gpt-4o",
        messages: vec![
            OpenAiMessage { role: "system", content: "You are an expert algorithmic trader. Always respond with valid JSON only." },
            OpenAiMessage { role: "user",   content: prompt },
        ],
    };

    debug!("Calling OpenAI API...");

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&config.api_key)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("OpenAI API request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API error {status}: {text}");
    }

    let data: OpenAiResponse = resp.json().await.context("OpenAI response parse error")?;

    data.choices.into_iter().next()
        .and_then(|c| c.message.content)
        .context("OpenAI returned empty content")
}
