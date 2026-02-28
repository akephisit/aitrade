-- Antigravity — PostgreSQL Schema
-- Migration 001: Initial Tables

-- ── Trade Records ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS trade_records (
    trade_id        UUID            PRIMARY KEY,
    strategy_id     UUID            NOT NULL,
    symbol          VARCHAR(20)     NOT NULL,
    direction       VARCHAR(10)     NOT NULL,    -- 'BUY' | 'SELL'
    entry_price     NUMERIC(20, 5)  NOT NULL,
    lot_size        NUMERIC(10, 4)  NOT NULL,
    take_profit     NUMERIC(20, 5)  NOT NULL,
    stop_loss       NUMERIC(20, 5)  NOT NULL,
    mt5_ticket      BIGINT,
    status          VARCHAR(20)     NOT NULL,    -- 'PENDING' | 'CONFIRMED' | 'FAILED'
    status_message  TEXT,
    fired_at        TIMESTAMPTZ     NOT NULL,
    created_at      TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trade_records_symbol    ON trade_records(symbol);
CREATE INDEX IF NOT EXISTS idx_trade_records_fired_at  ON trade_records(fired_at DESC);
CREATE INDEX IF NOT EXISTS idx_trade_records_status    ON trade_records(status);

-- ── Risk Events ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS risk_events (
    id          BIGSERIAL   PRIMARY KEY,
    event_type  VARCHAR(50) NOT NULL,   -- 'KILL_SWITCH_ON' | 'KILL_SWITCH_OFF' | 'AUTO_KILL' | 'COOLDOWN'
    details     TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_events_type ON risk_events(event_type);

-- ── Strategy Log ──────────────────────────────────────────────────────────────
-- บันทึกทุก Strategy ที่ OpenClaw เคยส่งมา (สำหรับ Backtesting analysis)
CREATE TABLE IF NOT EXISTS strategy_log (
    strategy_id     UUID            PRIMARY KEY,
    symbol          VARCHAR(20)     NOT NULL,
    direction       VARCHAR(10)     NOT NULL,
    entry_zone_low  NUMERIC(20, 5)  NOT NULL,
    entry_zone_high NUMERIC(20, 5)  NOT NULL,
    take_profit     NUMERIC(20, 5)  NOT NULL,
    stop_loss       NUMERIC(20, 5)  NOT NULL,
    lot_size        NUMERIC(10, 4)  NOT NULL,
    rationale       TEXT,
    created_at      TIMESTAMPTZ     NOT NULL,
    expires_at      TIMESTAMPTZ,
    received_at     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_strategy_log_symbol     ON strategy_log(symbol);
CREATE INDEX IF NOT EXISTS idx_strategy_log_created_at ON strategy_log(created_at DESC);
