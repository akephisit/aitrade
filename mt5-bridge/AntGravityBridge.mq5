//+------------------------------------------------------------------+
//|                                          AntGravityBridge.mq5   |
//|           Antigravity Trading System — MetaTrader 5 Bridge      |
//|  v2.0 — เพิ่ม OnTradeTransaction (Position Close) + RSI Sender  |
//+------------------------------------------------------------------+
#property copyright "Antigravity Team"
#property version   "2.00"
#property strict

#include <Trade\Trade.mqh>
#include <Trade\DealInfo.mqh>
#include <Indicators\Oscilators.mqh>

//── Input Parameters ──────────────────────────────────────────────────────────
input string BackendURL    = "http://127.0.0.1:3000";  // Antigravity backend URL
input int    RSI_Period    = 14;                        // RSI Period
input int    MA_Fast       = 20;                        // MA Fast Period
input int    MA_Slow       = 50;                        // MA Slow Period
input string ApiKey        = "";                        // X-API-Key (ถ้าตั้งไว้ใน backend)
input int    TimeoutMs     = 5000;                      // HTTP Timeout (ms)

//── Global Variables ──────────────────────────────────────────────────────────
CTrade g_trade;
int    g_rsi_handle  = INVALID_HANDLE;
int    g_ma20_handle = INVALID_HANDLE;
int    g_ma50_handle = INVALID_HANDLE;

//── Init ───────────────────────────────────────────────────────────────────────
int OnInit() {
    // สร้าง Indicator Handles (สร้างครั้งเดียว ไม่สร้างใหม่ทุก Tick)
    g_rsi_handle  = iRSI(_Symbol,  PERIOD_CURRENT, RSI_Period, PRICE_CLOSE);
    g_ma20_handle = iMA(_Symbol,   PERIOD_CURRENT, MA_Fast, 0, MODE_EMA, PRICE_CLOSE);
    g_ma50_handle = iMA(_Symbol,   PERIOD_CURRENT, MA_Slow, 0, MODE_EMA, PRICE_CLOSE);

    if (g_rsi_handle == INVALID_HANDLE) {
        Print("❌ RSI handle failed");
        return INIT_FAILED;
    }

    Print("✅ AntGravityBridge v2.0 initialized | Symbol: ", _Symbol,
          " | Backend: ", BackendURL);
    return INIT_SUCCEEDED;
}

//── DeInit ─────────────────────────────────────────────────────────────────────
void OnDeinit(const int reason) {
    if (g_rsi_handle  != INVALID_HANDLE) IndicatorRelease(g_rsi_handle);
    if (g_ma20_handle != INVALID_HANDLE) IndicatorRelease(g_ma20_handle);
    if (g_ma50_handle != INVALID_HANDLE) IndicatorRelease(g_ma50_handle);
    Print("AntGravityBridge deinit | Reason: ", reason);
}

//── OnTick ─────────────────────────────────────────────────────────────────────
void OnTick() {
    MqlTick tick;
    if (!SymbolInfoTick(_Symbol, tick)) return;

    // ── ดึงค่า Indicators ──────────────────────────────────────────────────────
    double rsi_val  = GetIndicatorValue(g_rsi_handle);
    double ma20_val = GetIndicatorValue(g_ma20_handle);
    double ma50_val = GetIndicatorValue(g_ma50_handle);

    // ── สร้าง JSON payload ──────────────────────────────────────────────────────
    string payload = StringFormat(
        "{"
        "\"symbol\":\"%s\","
        "\"bid\":%.5f,"
        "\"ask\":%.5f,"
        "\"volume\":%.2f,"
        "\"time\":\"%s\","
        "\"rsi_14\":%.4f,"
        "\"ma_20\":%.5f,"
        "\"ma_50\":%.5f"
        "}",
        _Symbol,
        tick.bid, tick.ask, (double)tick.volume,
        TimeToString(tick.time, TIME_DATE | TIME_SECONDS) + "Z",
        rsi_val,
        ma20_val,
        ma50_val
    );

    // ── POST ไป Backend ────────────────────────────────────────────────────────
    string response = HttpPost(BackendURL + "/api/mt5/tick", payload);
    if (response == "") return;

    // ── Parse Response ─────────────────────────────────────────────────────────
    if (StringFind(response, "\"TRADE_TRIGGERED\"") >= 0) {
        HandleTradeSignal(response, tick);
    } else if (StringFind(response, "\"MODIFY_POSITION\"") >= 0) {
        HandleModifySignal(response);
    }
}

//── OnTradeTransaction — ตรวจจับเมื่อ MT5 ปิด Position (TP / SL / Manual) ────
void OnTradeTransaction(
    const MqlTradeTransaction& trans,
    const MqlTradeRequest&     request,
    const MqlTradeResult&      result
) {
    // ต้องการแค่ DEAL_ADD (เมื่อมี Deal ใหม่ = position ถูกปิด)
    if (trans.type != TRADE_TRANSACTION_DEAL_ADD) return;

    CDealInfo deal;
    if (!deal.Ticket(trans.deal)) return;

    // เฉพาะ Deal ที่เป็นการปิด Position (ENTRY_OUT) หรือ Reverse (ENTRY_INOUT)
    ENUM_DEAL_ENTRY entry = deal.Entry();
    if (entry != DEAL_ENTRY_OUT && entry != DEAL_ENTRY_INOUT) return;

    // ── หาสาเหตุที่ปิด ────────────────────────────────────────────────────────
    string close_reason = "MANUAL";
    switch (deal.Reason()) {
        case DEAL_REASON_TP:     close_reason = "TP";     break;
        case DEAL_REASON_SL:     close_reason = "SL";     break;
        case DEAL_REASON_EXPERT: close_reason = "EXPERT";  break;
        case DEAL_REASON_CLIENT: close_reason = "MANUAL";  break;
        default:                 close_reason = "OTHER";   break;
    }

    double close_price = deal.Price();
    double profit      = deal.Profit();
    long   ticket      = deal.Ticket();
    string symbol      = deal.Symbol();

    Print("📤 Position closed | ", close_reason, " | Price: ", close_price,
          " | Profit: ", profit, " | Ticket: ", ticket);

    // ── ส่งไปยัง Backend ───────────────────────────────────────────────────────
    NotifyPositionClose(symbol, ticket, close_price, profit, close_reason);
}

//── Helpers ─────────────────────────────────────────────────────────────────────

double GetIndicatorValue(int handle) {
    if (handle == INVALID_HANDLE) return 0.0;
    double buffer[];
    if (CopyBuffer(handle, 0, 0, 1, buffer) <= 0) return 0.0;
    return buffer[0];
}

string BuildHeaders() {
    string headers = "Content-Type: application/json\r\n";
    if (ApiKey != "") {
        headers += "X-API-Key: " + ApiKey + "\r\n";
    }
    return headers;
}

string HttpPost(string url, string body) {
    char   req[];
    char   res[];
    string res_headers;
    StringToCharArray(body, req, 0, StringLen(body));

    int status = WebRequest(
        "POST", url,
        BuildHeaders(), TimeoutMs,
        req, res, res_headers
    );

    if (status == 200 || status == 201) {
        return CharArrayToString(res);
    }

    if (status == -1) {
        Print("❌ WebRequest failed: URL not whitelisted? | URL: ", url);
    } else {
        Print("⚠️ Backend returned HTTP ", status, " | URL: ", url);
    }
    return "";
}

void NotifyPositionClose(string symbol, long ticket, double close_price,
                          double profit, string close_reason) {
    string payload = StringFormat(
        "{"
        "\"mt5_ticket\":%d,"
        "\"symbol\":\"%s\","
        "\"close_price\":%.5f,"
        "\"profit_pips\":%.4f,"
        "\"close_reason\":\"%s\""
        "}",
        ticket, symbol, close_price, profit, close_reason
    );

    string response = HttpPost(BackendURL + "/api/mt5/position-close", payload);
    if (StringFind(response, "\"ok\":true") >= 0) {
        Print("✅ Backend notified: position closed | ", close_reason);
    } else {
        Print("⚠️ Backend position-close notification failed | Response: ", response);
    }
}

void HandleTradeSignal(string response, const MqlTick& tick) {
    // Parse direction
    string direction = "BUY";
    if (StringFind(response, "\"direction\":\"SELL\"") >= 0) direction = "SELL";
    if (StringFind(response, "\"direction\":\"Sell\"") >= 0) direction = "SELL";

    // Parse TP/SL/Lots
    double tp   = ParseDouble(response, "\"tp\":");
    double sl   = ParseDouble(response, "\"sl\":");
    double lots = ParseDouble(response, "\"lot_size\":");
    if (lots <= 0) lots = 0.01;

    Print("🎯 Trade signal received | ", direction,
          " | TP: ", tp, " | SL: ", sl, " | Lots: ", lots);

    MqlTradeRequest  req  = {};
    MqlTradeResult   res  = {};

    req.action    = TRADE_ACTION_DEAL;
    req.symbol    = _Symbol;
    req.volume    = lots;
    req.type      = (direction == "BUY") ? ORDER_TYPE_BUY : ORDER_TYPE_SELL;
    req.price     = (direction == "BUY") ? tick.ask : tick.bid;
    req.sl        = sl;
    req.tp        = tp;
    req.deviation = 10;
    req.magic     = 202600;
    req.comment   = "Antigravity";
    req.type_filling = ORDER_FILLING_IOC;

    if (!OrderSend(req, res)) {
        int err = GetLastError();
        Print("❌ OrderSend failed | Error: ", err, " | Retcode: ", res.retcode);
    } else {
        Print("✅ Order sent | Ticket: ", res.order,
              " | Price: ", res.price, " | Retcode: ", res.retcode);
    }
}

void HandleModifySignal(string response) {
    long   ticket = (long)ParseDouble(response, "\"mt5_ticket\":");
    double new_sl = ParseDouble(response, "\"new_sl\":");

    if (ticket <= 0) return;

    if (PositionSelectByTicket(ticket)) {
        double current_tp = PositionGetDouble(POSITION_TP);
        
        MqlTradeRequest req = {};
        MqlTradeResult  res = {};
        
        req.action   = TRADE_ACTION_SLTP;
        req.position = ticket;
        req.symbol   = PositionGetString(POSITION_SYMBOL);
        req.sl       = new_sl;
        req.tp       = current_tp;

        if (!OrderSend(req, res)) {
            Print("❌ ModifyPosition failed | Ticket: ", ticket, " | Error: ", GetLastError());
        } else {
            Print("🛡️ SL Modified (Break-Even/Trailing) | Ticket: ", ticket, " | New SL: ", new_sl);
        }
    } else {
        Print("⚠️ Cannot select position for modification | Ticket: ", ticket);
    }
}

double ParseDouble(string json, string key) {
    int pos = StringFind(json, key);
    if (pos < 0) return 0.0;
    string val = StringSubstr(json, pos + StringLen(key));
    // หยุดที่ , หรือ }
    int end1 = StringFind(val, ",");
    int end2 = StringFind(val, "}");
    int end  = (end1 < 0) ? end2 : (end2 < 0) ? end1 : MathMin(end1, end2);
    if (end > 0) val = StringSubstr(val, 0, end);
    return StringToDouble(val);
}
//+------------------------------------------------------------------+
