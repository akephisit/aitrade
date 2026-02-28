# Antigravity — Automated Trading System

> High-performance algorithmic trading backend built with **Rust + Axum**, AI-powered strategy generation via **OpenClaw** (Claude/GPT-4o), real-time monitoring via **SvelteKit**, and MetaTrader 5 integration.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  OpenClaw (AI Agent)          Brain Loop (every N minutes)          │
│  Claude 3.5 / GPT-4o    ────► POST /api/brain/strategy              │
└─────────────────────────────┬───────────────────────────────────────┘
                              │ ActiveStrategy { zone, tp, sl, lots }
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│              Antigravity Backend (Axum)                             │
│                                                                     │
│  POST /api/mt5/tick ──► [Reflex Engine]                             │
│                              │                                      │
│                         4-Layer Confirmation:                       │
│                         [1] Spread Check                            │
│                         [2] Zone Probe (Bounce Pattern)             │
│                         [3] Zone Dwell (≥ N ticks)                  │
│                         [4] RSI Filter (optional)                   │
│                              │                                      │
│                         [Risk Manager] ──► Kill Switch / Limits     │
│                              │                                      │
│                         POST to MT5 EA → OrderSend()               │
│                              │                                      │
│  POST /api/mt5/position-close ◄── OnTradeTransaction (TP/SL)        │
│                              │                                      │
│  WebSocket /ws/monitor ──────► Real-time events to Dashboard        │
└─────────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────────┐
│  MetaTrader 5 (AntGravityBridge.mq5)                                │
│  OnTick(): POST tick + RSI + MA data                                │
│  OnTradeTransaction(): POST position-close when TP/SL hit           │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Quick Start (Docker)

```bash
# 1. Clone
git clone <repo> && cd aitrade

# 2. ตั้งค่า Environment
cp backend/.env.example .env
# แก้ไข .env:
#   AI_API_KEY=sk-ant-...
#   SYMBOL=BTCUSD

# 3. รัน
docker compose up -d

# Services:
#   PostgreSQL  → localhost:5432
#   Backend     → http://localhost:3000
#   Dashboard   → http://localhost:3001
```

---

## Manual Setup

### Prerequisites
- Rust 1.75+
- Node.js 20+
- MetaTrader 5 (Windows)

### 1. Backend

```bash
cd backend
cp .env.example .env
# แก้ไข .env ตามต้องการ

cargo run
# → Server starts on http://0.0.0.0:3000
```

### 2. OpenClaw (AI Agent)

```bash
cd openclaw
cp .env.example .env
# ตั้ง AI_API_KEY และ AI_PROVIDER (claude หรือ openai)

cargo run
# → Brain Loop starts, calls AI every BRAIN_INTERVAL_SECS
```

### 3. Dashboard

```bash
cd frontend
npm install
npm run dev
# → Dashboard on http://localhost:5173
```

### 4. MetaTrader 5 EA

1. Copy `mt5-bridge/AntGravityBridge.mq5` → MT5 `Experts/` folder
2. Compile ใน MetaEditor
3. MT5 → Tools → Options → Expert Advisors → Allow WebRequest
4. เพิ่ม URL: `http://127.0.0.1:3000`
5. Attach EA กับ Chart ของ Symbol ที่ต้องการ

---

## Environment Variables

### Backend (`backend/.env`)

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `MT5_BASE_URL` | `http://localhost:8081` | MT5 EA HTTP endpoint |
| `API_KEY` | _(empty = dev mode)_ | API Key สำหรับ Production |
| `RUST_LOG` | `antigravity=debug` | Log level |
| `CONFIRM_MAX_SPREAD` | `50.0` | Spread สูงสุด (price units) |
| `CONFIRM_REQUIRE_PROBE` | `true` | ต้องมี Zone Probe ก่อนเข้า |
| `CONFIRM_MIN_ZONE_TICKS` | `2` | Ticks ขั้นต่ำใน Zone |
| `CONFIRM_PROBE_LOOKBACK` | `15` | Ticks ย้อนหลังสำหรับ Probe |
| `CONFIRM_RSI_OVERBOUGHT` | `70.0` | RSI Overbought (BUY ห้าม ≥ นี้) |
| `CONFIRM_RSI_OVERSOLD` | `30.0` | RSI Oversold (SELL ห้าม ≤ นี้) |
| `RISK_MAX_TRADES_PER_DAY` | `10` | Trade สูงสุดต่อวัน |
| `RISK_MAX_CONSECUTIVE_FAILS` | `3` | Fail ติดกันสูงสุดก่อน Auto-Kill |
| `RISK_COOLDOWN_SECS` | `300` | พักหลัง Fail (วินาที) |

### OpenClaw (`openclaw/.env`)

| Variable | Default | Description |
|----------|---------|-------------|
| `AI_PROVIDER` | `claude` | `claude` หรือ `openai` |
| `AI_API_KEY` | _(required)_ | Anthropic หรือ OpenAI API Key |
| `SYMBOL` | `BTCUSD` | Symbol ที่ต้องการ Trade |
| `AITRADE_URL` | `http://localhost:3000` | Backend URL |
| `BRAIN_INTERVAL_SECS` | `300` | ความถี่ Brain Loop (วินาที) |
| `STRATEGY_TTL_MIN` | `15` | Strategy หมดอายุ (นาที) |

---

## API Reference

### Brain Loop

```bash
# POST Strategy (from OpenClaw)
POST /api/brain/strategy
Content-Type: application/json
X-API-Key: <key>   # ถ้าตั้ง API_KEY

# GET current strategy
GET /api/brain/strategy

# DELETE strategy (disarm)
DELETE /api/brain/strategy
```

### Reflex Loop (MT5 EA)

```bash
# POST Tick
POST /api/mt5/tick
{ "symbol":"BTCUSD", "bid":67000.0, "ask":67002.0,
  "volume":1.5, "time":"2026-02-28T07:00:00Z",
  "rsi_14":55.3, "ma_20":66950.0, "ma_50":66800.0 }

# POST Position Close (when TP/SL hit)
POST /api/mt5/position-close
{ "mt5_ticket":12345, "symbol":"BTCUSD",
  "close_price":67200.0, "profit_pips":10.5, "close_reason":"TP" }

# GET Health
GET /api/mt5/health
```

### Monitor

```bash
# WebSocket (real-time events)
ws://localhost:3000/ws/monitor

# REST
GET /api/monitor/position   # current open position
GET /api/monitor/history    # trade history
GET /api/monitor/stats      # server statistics
```

### Risk Management

```bash
# Kill Switch ON
POST /api/risk/kill
{ "reason": "Emergency stop" }

# Kill Switch OFF (re-arm)
POST /api/risk/rearm

# Status
GET /api/risk/status
```

### Backtesting

```bash
POST /api/backtest
Content-Type: application/json

{
  "strategy": {
    "symbol": "BTCUSD",
    "direction": "Buy",
    "entry_zone": { "low": 67000, "high": 67050 },
    "take_profit": 67300,
    "stop_loss": 66800,
    "lot_size": 0.01,
    ...
  },
  "ticks": [ ... ],
  "confirmation": {
    "max_spread": 50,
    "require_zone_probe": true,
    "min_zone_ticks": 2
  }
}
```

---

## WebSocket Events

| Event | Description |
|-------|-------------|
| `SNAPSHOT` | Initial state when dashboard connects |
| `STRATEGY_UPDATED` | New strategy from OpenClaw |
| `STRATEGY_CLEARED` | Strategy cleared after trade fired |
| `TRADE_FIRING` | Reflex Engine triggered, sending to MT5 |
| `POSITION_OPENED` | MT5 confirmed order, position is open |
| `POSITION_CLOSED` | MT5 hit TP/SL, position closed |
| `TRADE_FAILED` | MT5 rejected or unreachable |
| `RISK_KILLED` | Kill switch activated |
| `SERVER_STATS` | Periodic tick/trade count update |

---

## Project Structure

```
aitrade/
├── backend/              Rust · Axum Backend
│   ├── src/
│   │   ├── engine/       reflex.rs, confirmation.rs, executor.rs
│   │   ├── models/       tick.rs, strategy.rs, position.rs
│   │   ├── routes/       mt5.rs, brain.rs, monitor.rs, risk.rs, backtest.rs
│   │   ├── auth.rs       API Key middleware
│   │   ├── risk.rs       Risk Manager
│   │   ├── state.rs      SharedState (Arc<AppState>)
│   │   └── events.rs     WebSocket event types
│   ├── migrations/       PostgreSQL migration SQL
│   └── Dockerfile
│
├── openclaw/             Rust · AI Brain Agent
│   ├── src/
│   │   ├── ai.rs         Claude 3.5 + GPT-4o API
│   │   ├── strategy.rs   Parse AI → ActiveStrategy
│   │   └── poster.rs     POST to backend
│   └── Dockerfile
│
├── frontend/             SvelteKit · Dashboard
│   └── src/
│       ├── lib/stores.ts WebSocket + API stores
│       └── routes/+page.svelte  Trading Dashboard
│
├── mt5-bridge/           MQL5 · Expert Advisor
│   └── AntGravityBridge.mq5
│       ├── OnTick()         POST tick + RSI + MA
│       └── OnTradeTransaction()  POST position-close
│
└── docker-compose.yml    Production deployment
```

---

## Confirmation Engine (4 Layers)

```
Price enters Entry Zone
        │
        ▼
[1] Spread ≤ max_spread          → ป้องกัน News/High Volatility
        │
        ▼
[2] Zone Probe detected          → ราคาเคย Test นอก Zone ก่อน
    BUY:  mid < zone_low  (Support bounce)
    SELL: mid > zone_high (Resistance rejection)
        │
        ▼
[3] Zone Dwell ≥ N ticks         → ไม่ใช่แค่ Wick ผ่าน
        │
        ▼
[4] RSI in range (if provided)   → ไม่ Overbought/Oversold
    BUY:  RSI < 70
    SELL: RSI > 30
        │
        ▼
    🎯 FIRE TRADE
```

---

## License

MIT
