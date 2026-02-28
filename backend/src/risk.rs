//! # risk — Risk Management Engine
//!
//! ชั้นกั้นสุดท้ายก่อนยิง Order — ป้องกันพอร์ตล้าง
//!
//! ## ชั้นการป้องกัน
//! 1. **Kill Switch**       — หยุดระบบฉุกเฉิน (manual หรือ auto)
//! 2. **Max Trades/Day**    — จำกัดจำนวน Trade ต่อวัน
//! 3. **Auto-Kill**         — หยุดอัตโนมัติเมื่อ Fail ติดต่อกัน N ครั้ง
//! 4. **Cooldown**          — พักหลัง Fail ก่อน Trade ใหม่

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// จำนวน Trade สูงสุดต่อวัน (0 = ไม่จำกัด)
    pub max_trades_per_day: u32,
    /// Fail ติดต่อกันกี่ครั้งถึง Auto-Kill (0 = ไม่ Auto-Kill)
    pub max_consecutive_failures: u32,
    /// พักหลังจาก Fail กี่วินาที
    pub cooldown_secs_after_failure: u64,
}

impl RiskConfig {
    pub fn from_env() -> Self {
        Self {
            max_trades_per_day:         env_u32("RISK_MAX_TRADES_PER_DAY", 10),
            max_consecutive_failures:   env_u32("RISK_MAX_CONSECUTIVE_FAILS", 3),
            cooldown_secs_after_failure: env_u64("RISK_COOLDOWN_SECS", 300),
        }
    }
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

// ─── Internal State ───────────────────────────────────────────────────────────

#[derive(Debug)]
struct RiskInner {
    is_killed:            bool,
    kill_reason:          Option<String>,
    trades_today:         u32,
    consecutive_failures: u32,
    last_failure_at:      Option<DateTime<Utc>>,
    last_trade_at:        Option<DateTime<Utc>>,
    daily_reset_date:     NaiveDate,
}

// ─── Status (for Dashboard / API) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RiskStatus {
    pub is_killed:            bool,
    pub kill_reason:          Option<String>,
    pub trades_today:         u32,
    pub consecutive_failures: u32,
    pub last_trade_at:        Option<DateTime<Utc>>,
    pub in_cooldown:          bool,
    pub cooldown_ends_at:     Option<DateTime<Utc>>,
    pub config: RiskConfigSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskConfigSnapshot {
    pub max_trades_per_day:         u32,
    pub max_consecutive_failures:   u32,
    pub cooldown_secs_after_failure: u64,
}

// ─── Decision ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RiskDecision {
    Approved,
    Blocked(String),
}

// ─── Risk Manager ─────────────────────────────────────────────────────────────

pub struct RiskManager {
    inner:  Arc<RwLock<RiskInner>>,
    config: Arc<RiskConfig>,
}

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RiskInner {
                is_killed:            false,
                kill_reason:          None,
                trades_today:         0,
                consecutive_failures: 0,
                last_failure_at:      None,
                last_trade_at:        None,
                daily_reset_date:     Utc::now().date_naive(),
            })),
            config: Arc::new(config),
        }
    }

    // ─── Pre-Trade Check (เรียกก่อนยิง Order ทุกครั้ง) ──────────────────────

    pub async fn pre_trade_check(&self) -> RiskDecision {
        let mut inner = self.inner.write().await;

        // Daily reset
        let today = Utc::now().date_naive();
        if today > inner.daily_reset_date {
            inner.trades_today    = 0;
            inner.daily_reset_date = today;
            info!("📅 Risk: daily counters reset");
        }

        // [1] Kill switch
        if inner.is_killed {
            return RiskDecision::Blocked(format!(
                "Kill switch active: {}",
                inner.kill_reason.as_deref().unwrap_or("manual activation")
            ));
        }

        // [2] Cooldown หลัง Fail
        if let Some(fail_time) = inner.last_failure_at {
            let elapsed  = Utc::now().signed_duration_since(fail_time);
            let cooldown = chrono::Duration::seconds(self.config.cooldown_secs_after_failure as i64);
            if elapsed < cooldown {
                let remaining = (cooldown - elapsed).num_seconds();
                return RiskDecision::Blocked(format!(
                    "Cooldown: {remaining}s remaining after last failure"
                ));
            }
        }

        // [3] Max trades per day
        if self.config.max_trades_per_day > 0
            && inner.trades_today >= self.config.max_trades_per_day
        {
            return RiskDecision::Blocked(format!(
                "Daily trade limit reached: {}/{}", inner.trades_today,
                self.config.max_trades_per_day
            ));
        }

        // [4] Consecutive failure auto-kill
        if self.config.max_consecutive_failures > 0
            && inner.consecutive_failures >= self.config.max_consecutive_failures
        {
            let reason = format!(
                "Auto-kill: {} consecutive execution failures",
                inner.consecutive_failures
            );
            inner.is_killed   = true;
            inner.kill_reason = Some(reason.clone());
            warn!("⛔ Risk auto-kill activated: {reason}");
            return RiskDecision::Blocked(reason);
        }

        // Approved — บันทึก
        inner.trades_today += 1;
        inner.last_trade_at = Some(Utc::now());
        info!(
            trades_today = inner.trades_today,
            max          = self.config.max_trades_per_day,
            "✅ Risk approved"
        );

        RiskDecision::Approved
    }

    // ─── Trade Result Recording ───────────────────────────────────────────────

    /// เรียกเมื่อ MT5 ยืนยัน Order สำเร็จ
    pub async fn record_success(&self) {
        let mut inner = self.inner.write().await;
        let prev = inner.consecutive_failures;
        inner.consecutive_failures = 0;
        if prev > 0 {
            info!("Risk: consecutive_failures reset (was {prev})");
        }
    }

    /// เรียกเมื่อ Order Fail
    pub async fn record_failure(&self) {
        let mut inner = self.inner.write().await;
        inner.consecutive_failures += 1;
        inner.last_failure_at = Some(Utc::now());
        warn!(
            consecutive = inner.consecutive_failures,
            max         = self.config.max_consecutive_failures,
            "⚠️ Risk: execution failure recorded"
        );
    }

    // ─── Manual Controls ─────────────────────────────────────────────────────

    /// ปิดระบบฉุกเฉิน
    pub async fn kill(&self, reason: &str) {
        let mut inner = self.inner.write().await;
        inner.is_killed   = true;
        inner.kill_reason = Some(reason.to_string());
        warn!(reason, "⛔ KILL SWITCH ACTIVATED");
    }

    /// เปิดระบบอีกครั้ง (หลังแก้ไขปัญหาแล้ว)
    pub async fn rearm(&self) {
        let mut inner = self.inner.write().await;
        inner.is_killed            = false;
        inner.kill_reason          = None;
        inner.consecutive_failures = 0;
        inner.last_failure_at      = None;
        info!("✅ KILL SWITCH DEACTIVATED — system re-armed");
    }

    // ─── Status ───────────────────────────────────────────────────────────────

    pub async fn status(&self) -> RiskStatus {
        let inner = self.inner.read().await;
        let cooldown_ends = inner.last_failure_at.map(|t| {
            t + chrono::Duration::seconds(self.config.cooldown_secs_after_failure as i64)
        });
        let in_cooldown = cooldown_ends.map(|end| Utc::now() < end).unwrap_or(false);

        RiskStatus {
            is_killed:            inner.is_killed,
            kill_reason:          inner.kill_reason.clone(),
            trades_today:         inner.trades_today,
            consecutive_failures: inner.consecutive_failures,
            last_trade_at:        inner.last_trade_at,
            in_cooldown,
            cooldown_ends_at:     if in_cooldown { cooldown_ends } else { None },
            config: RiskConfigSnapshot {
                max_trades_per_day:          self.config.max_trades_per_day,
                max_consecutive_failures:    self.config.max_consecutive_failures,
                cooldown_secs_after_failure: self.config.cooldown_secs_after_failure,
            },
        }
    }
}
