#!/usr/bin/env bash
# ── Antigravity — End-to-End Demo Script ───────────────────────────────────────
# รัน demo ทั้งระบบโดยไม่ต้องมี MT5 หรือ AI API Key
# จำลอง: ส่ง Strategy → ส่ง Ticks → ดู Trade Fire → Position Close
#
# Usage:
#   chmod +x demo.sh
#   ./demo.sh
#
# ต้องรัน backend ก่อน:
#   cd backend && cargo run

set -e

BASE_URL="${BACKEND_URL:-http://localhost:3000}"
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log()    { echo -e "${CYAN}[DEMO]${NC} $*"; }
ok()     { echo -e "${GREEN}[✓]${NC} $*"; }
warn()   { echo -e "${YELLOW}[!]${NC} $*"; }
error()  { echo -e "${RED}[✗]${NC} $*"; exit 1; }
header() { echo -e "\n${BLUE}══════════════════════════════════════════${NC}"; echo -e "${BLUE}  $*${NC}"; echo -e "${BLUE}══════════════════════════════════════════${NC}"; }

# ── 0. ตรวจสอบ Backend ─────────────────────────────────────────────────────────
header "Step 0: Health Check"
HEALTH=$(curl -sf "${BASE_URL}/api/mt5/health" 2>/dev/null || echo "FAIL")
if echo "$HEALTH" | grep -q '"ok":true'; then
    ok "Backend is running at ${BASE_URL}"
else
    error "Backend not reachable at ${BASE_URL}. Run: cd backend && cargo run"
fi

# ── 1. ตรวจสอบ Risk Status ─────────────────────────────────────────────────────
header "Step 1: Risk Status"
RISK=$(curl -sf "${BASE_URL}/api/risk/status" | python3 -m json.tool 2>/dev/null || \
       curl -sf "${BASE_URL}/api/risk/status")
echo "$RISK"
ok "Risk system initialized"

# ── 2. ส่ง Active Strategy ─────────────────────────────────────────────────────
header "Step 2: Push Strategy (BUY BTCUSD)"
log "Sending strategy: BUY BTCUSD | Zone: 67000-67050 | TP: 67300 | SL: 66800"

STRATEGY_RESP=$(curl -sf -X POST "${BASE_URL}/api/brain/strategy" \
    -H "Content-Type: application/json" \
    -d '{
        "strategy_id": "00000000-0000-0000-0000-000000000001",
        "symbol": "BTCUSD",
        "direction": "BUY",
        "entry_zone": { "low": 67000.0, "high": 67050.0 },
        "take_profit": 67300.0,
        "stop_loss":   66800.0,
        "opposing_zone": { "low": 67250.0, "high": 67280.0 },
        "lot_size":    0.01,
        "rationale":   "Demo: Support bounce at 67000",
        "created_at":  "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'",
        "expires_at":  null
    }')

echo "$STRATEGY_RESP" | grep -q '"ok":true' && ok "Strategy posted!" || warn "Response: $STRATEGY_RESP"

sleep 0.5

# ── 3. ส่ง Ticks นอก Zone (Build Buffer) ──────────────────────────────────────
header "Step 3: Build Tick Buffer (Zone Probe)"
log "Sending ticks BELOW zone (simulating support test)..."

send_tick() {
    local BID=$1 ASK=$2 RSI=$3
    curl -sf -X POST "${BASE_URL}/api/mt5/tick" \
        -H "Content-Type: application/json" \
        -d "{
            \"symbol\": \"BTCUSD\",
            \"bid\": $BID,
            \"ask\": $ASK,
            \"volume\": 1.5,
            \"time\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
            \"rsi_14\": $RSI,
            \"ma_20\": 66950.0,
            \"ma_50\": 66800.0
        }" > /dev/null
    echo -n "."
}

# Ticks ต่ำกว่า Zone (Zone Probe)
for i in 66995 66985 66975 66970 66980 66990; do
    send_tick $i $((i+2)) 42.0
    sleep 0.1
done
echo ""
ok "Ticks below zone_low sent (Zone Probe will be detected)"

sleep 0.3

# ── 4. ส่ง Ticks เข้า Zone (Wick Rejection Builder) ────────────────────────
header "Step 4: Enter Entry Zone + SMC Wick Rejection Formation"
log "Sending 5+ ticks INTO zone to form a rejection candle (M1)..."

# จำลองแท่งเทียน: 
# Open: 67035 (นอกโซน)
# Drop: 66990 (กวาดสภาพคล่อง ลึกสุด)
# Climb: 67010 -> 67020 -> 67025 (ราคากลับขึ้นมา ปิดใน/ใกล้โซน ทิ้งไส้ยาว)
for i in 67035 66990 67010 67020 67025 67026; do
    RESP=$(curl -sf -X POST "${BASE_URL}/api/mt5/tick" \
        -H "Content-Type: application/json" \
        -d "{
            \"symbol\": \"BTCUSD\",
            \"bid\": $i,
            \"ask\": $((i+2)),
            \"volume\": 2.1,
            \"time\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
            \"rsi_14\": 55.0,
            \"ma_20\": 66950.0,
            \"ma_50\": 66800.0
        }")

    if echo "$RESP" | grep -q "TRADE_TRIGGERED"; then
        echo ""
        ok "🎯 TRADE TRIGGERED at tick $i!"
        echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  Direction: {d.get(\"direction\",\"?\")} | Entry: {d.get(\"entry_price\",\"?\")} | TP: {d.get(\"tp\",\"?\")} | SL: {d.get(\"sl\",\"?\")} | Ticket: {d.get(\"mt5_ticket\",\"?\")}') " 2>/dev/null || echo "  $RESP"
        TRADE_FIRED=1
        break
    elif echo "$RESP" | grep -q "RISK_BLOCKED"; then
        echo ""
        warn "Risk blocked: $(echo $RESP | grep -o '"reason":"[^"]*"')"
    else
        echo -n "."
    fi
    sleep 0.1
done
echo ""

if [ -z "$TRADE_FIRED" ]; then
    warn "Trade not triggered (confirmation check may need more ticks)"
    warn "This is expected — try adjusting CONFIRM_MIN_ZONE_TICKS=1 in .env for demo"
fi

# ── 4.5 จำลอง Opposing Zone Bailout ──────────────────────────────────────────
header "Step 4.5: Opposing Zone Bailout Check"
log "Sending ticks to Opposing Zone (67250-67280)..."
RESP=$(curl -sf -X POST "${BASE_URL}/api/mt5/tick" \
    -H "Content-Type: application/json" \
    -d "{
        \"symbol\": \"BTCUSD\",
        \"bid\": 67260,
        \"ask\": 67262,
        \"volume\": 5.0,
        \"time\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
        \"rsi_14\": 75.0,
        \"ma_20\": 66950.0,
        \"ma_50\": 66800.0
    }")

if echo "$RESP" | grep -q "CLOSE_POSITION"; then
    ok "⚔️ Bailout triggered! Price entered opposing zone (67260)."
else
    warn "Bailout not triggered. Response: $RESP"
fi

sleep 0.3

# ── 5. ดู Current State ────────────────────────────────────────────────────────
header "Step 5: Current State"

log "Strategy:"
curl -sf "${BASE_URL}/api/brain/strategy" | \
    python3 -c "import sys,json; d=json.load(sys.stdin); s=d.get('strategy'); print(f'  {s[\"direction\"] if s else \"None\"} {s[\"symbol\"] if s else \"\"}')" 2>/dev/null || \
    curl -sf "${BASE_URL}/api/brain/strategy"

log "Position:"
curl -sf "${BASE_URL}/api/monitor/position" | \
    python3 -c "import sys,json; d=json.load(sys.stdin); p=d.get('position'); print(f'  {p[\"direction\"] if p else \"FLAT\"} @ {p.get(\"entry_price\",\"-\") if p else \"\"}')" 2>/dev/null || \
    curl -sf "${BASE_URL}/api/monitor/position"

log "Trade History:"
curl -sf "${BASE_URL}/api/monitor/history" | \
    python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  {d[\"count\"]} trades in history')" 2>/dev/null || \
    curl -sf "${BASE_URL}/api/monitor/history"

# ── 6. จำลอง Position Close (TP Hit) ──────────────────────────────────────────
header "Step 6: Simulate Position Close (TP Hit)"
log "Simulating MT5 calling position-close..."

CLOSE_RESP=$(curl -sf -X POST "${BASE_URL}/api/mt5/position-close" \
    -H "Content-Type: application/json" \
    -d '{
        "mt5_ticket": null,
        "symbol": "BTCUSD",
        "close_price": 67305.0,
        "profit_pips": 10.5,
        "close_reason": "TP"
    }')

if echo "$CLOSE_RESP" | grep -q '"ok":true'; then
    ok "Position closed! Profit: +10.5 pips | Reason: TP"
else
    echo "Response: $CLOSE_RESP"
fi

sleep 0.3

# ── 7. ยืนยันว่า position ถูก clear ─────────────────────────────────────────
header "Step 7: Verify Reset"
POS_AFTER=$(curl -sf "${BASE_URL}/api/monitor/position")
if echo "$POS_AFTER" | grep -q '"position":null'; then
    ok "Position cleared — Reflex Loop re-armed! Ready for next trade."
else
    warn "Position may still be set: $POS_AFTER"
fi

# ── 8. Risk Status สุดท้าย ────────────────────────────────────────────────────
header "Step 8: Final Risk Status"
curl -sf "${BASE_URL}/api/risk/status" | \
    python3 -c "
import sys, json
d = json.load(sys.stdin).get('risk', {})
print(f'  Killed:    {d.get(\"is_killed\", \"?\")}')
print(f'  Trades:    {d.get(\"trades_today\", \"?\")}/{d.get(\"config\",{}).get(\"max_trades_per_day\",\"?\")}')
print(f'  Failures:  {d.get(\"consecutive_failures\", \"?\")}')
print(f'  Cooldown:  {d.get(\"in_cooldown\", \"?\")}')
" 2>/dev/null || curl -sf "${BASE_URL}/api/risk/status"

echo ""
header "Demo Complete! ✅"
echo -e "  Dashboard: ${CYAN}http://localhost:5173${NC}  (npm run dev in frontend/)"
echo -e "  Backend:   ${CYAN}http://localhost:3000${NC}"
echo -e ""
echo -e "  WebSocket events can be seen at: ${CYAN}ws://localhost:3000/ws/monitor${NC}"
echo ""
