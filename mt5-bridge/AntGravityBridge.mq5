//+------------------------------------------------------------------+
//| AntGravityBridge.mq5                                             |
//| MetaTrader 5 EA — Antigravity Trading System Bridge              |
//|                                                                  |
//| หน้าที่:                                                          |
//|   1. OnTick() → POST ราคาไปยัง aitrade /api/mt5/tick             |
//|   2. Response = TRADE_TRIGGERED → Execute Order ทันที            |
//|                                                                  |
//| วิธีติดตั้ง:                                                      |
//|   1. Copy ไฟล์นี้ไปที่ MT5 Data Folder/MQL5/Experts/             |
//|   2. Compile ใน MetaEditor (F7)                                  |
//|   3. Tools > Options > Expert Advisors:                          |
//|      ✅ Allow automated trading                                  |
//|      ✅ Allow WebRequest for listed URL:                         |
//|         http://127.0.0.1:3000                                    |
//|   4. Drag EA ลงบน Chart ของ Symbol ที่ต้องการ                    |
//+------------------------------------------------------------------+
#property copyright "Antigravity Trading System"
#property version   "1.00"
#property strict

#include <Trade\Trade.mqh>
#include <JAson.mqh>   // ต้องดาวน์โหลด JAson.mqh จาก MQL5 Market

//--- Input Parameters
input string   InpAitradeUrl    = "http://127.0.0.1:3000"; // aitrade Backend URL
input int      InpTimeoutMs     = 3000;                     // HTTP Timeout (ms)
input bool     InpSendEveryTick = true;                     // ส่งทุก Tick
input int      InpSendIntervalMs= 100;                      // ถ้าไม่ส่งทุก Tick ส่งทุก N ms
input ulong    InpMagicNumber   = 420001;                   // Magic Number ของ Antigravity

//--- Global Variables
CTrade         g_trade;
datetime       g_last_send_time = 0;
int            g_tick_count     = 0;

//+------------------------------------------------------------------+
//| Expert initialization                                            |
//+------------------------------------------------------------------+
int OnInit() {
   g_trade.SetExpertMagicNumber(InpMagicNumber);
   g_trade.SetDeviationInPoints(10);   // Slippage tolerance
   
   Print("AntGravityBridge started | Symbol: ", Symbol(), 
         " | Backend: ", InpAitradeUrl);
   
   return(INIT_SUCCEEDED);
}

//+------------------------------------------------------------------+
//| Expert deinitialization                                          |
//+------------------------------------------------------------------+
void OnDeinit(const int reason) {
   Print("AntGravityBridge stopped | Ticks sent: ", g_tick_count);
}

//+------------------------------------------------------------------+
//| OnTick — ส่งราคาไป aitrade ทุกครั้งที่ราคาเปลี่ยน               |
//+------------------------------------------------------------------+
void OnTick() {
   // Rate limiting (ถ้าไม่ต้องการส่งทุก Tick)
   if(!InpSendEveryTick) {
      if(GetTickCount() - (ulong)g_last_send_time < (ulong)InpSendIntervalMs)
         return;
   }
   
   MqlTick tick;
   if(!SymbolInfoTick(Symbol(), tick)) {
      Print("ERROR: Cannot get tick for ", Symbol());
      return;
   }
   
   // สร้าง JSON payload
   string payload = BuildTickPayload(tick);
   
   // ส่งไป aitrade และรับ Response
   string response = PostToAitrade("/api/mt5/tick", payload);
   
   if(response == "") return;   // HTTP Error
   
   g_tick_count++;
   
   // Parse Response
   HandleTickResponse(response, tick);
}

//+------------------------------------------------------------------+
//| สร้าง JSON Payload สำหรับ /api/mt5/tick                         |
//+------------------------------------------------------------------+
string BuildTickPayload(const MqlTick &tick) {
   datetime utc_time = tick.time;   // MT5 time ปกติเป็น server time
   
   string time_str = TimeToString(utc_time, TIME_DATE|TIME_SECONDS);
   StringReplace(time_str, ".", "-");          // 2025.01.01 → 2025-01-01
   StringReplace(time_str, " ", "T");          // 2025-01-01 12:00:00 → 2025-01-01T12:00:00
   time_str += "Z";                            // เพิ่ม UTC suffix
   
   string json = StringFormat(
      "{"
         "\"symbol\":\"%s\","
         "\"bid\":%.5f,"
         "\"ask\":%.5f,"
         "\"volume\":%.2f,"
         "\"time\":\"%s\""
      "}",
      Symbol(),
      tick.bid,
      tick.ask,
      tick.volume_real,
      time_str
   );
   
   return json;
}

//+------------------------------------------------------------------+
//| ส่ง HTTP POST ไป aitrade และคืน response body                    |
//+------------------------------------------------------------------+
string PostToAitrade(const string endpoint, const string body) {
   string url      = InpAitradeUrl + endpoint;
   string headers  = "Content-Type: application/json\r\n";
   char   data[];
   char   result[];
   string result_headers;
   
   StringToCharArray(body, data, 0, StringLen(body));
   
   int http_code = WebRequest(
      "POST",           // Method
      url,              // URL
      headers,          // Headers
      InpTimeoutMs,     // Timeout
      data,             // Request body
      result,           // Response body
      result_headers    // Response headers
   );
   
   if(http_code == -1) {
      int err = GetLastError();
      // Error 4060 = WebRequest ไม่ได้ Whitelist URL
      if(err == 4060)
         Print("ERROR: Add '", InpAitradeUrl, "' to Tools > Options > Expert Advisors > WebRequest URLs");
      else
         Print("HTTP Error: ", err, " | URL: ", url);
      return "";
   }
   
   if(http_code != 200) {
      Print("HTTP ", http_code, " from aitrade | endpoint: ", endpoint);
      return "";
   }
   
   return CharArrayToString(result);
}

//+------------------------------------------------------------------+
//| Parse Response จาก /api/mt5/tick และ Execute Order ถ้าจำเป็น    |
//+------------------------------------------------------------------+
void HandleTickResponse(const string response, const MqlTick &tick) {
   // ตรวจสอบว่ามี TRADE_TRIGGERED ไหม
   if(StringFind(response, "TRADE_TRIGGERED") == -1)
      return;   // NO_ACTION — จบ
   
   // Parse JSON response
   // Expected: {"action":"TRADE_TRIGGERED","direction":"BUY","entry_price":67032.0,"tp":67100.0,"sl":66900.0,...}
   string direction    = ExtractJsonString(response, "direction");
   double entry_price  = ExtractJsonDouble(response, "entry_price");
   double tp           = ExtractJsonDouble(response, "tp");
   double sl           = ExtractJsonDouble(response, "sl");
   
   Print("🎯 TRADE_TRIGGERED | direction=", direction, 
         " | entry=", entry_price, " | TP=", tp, " | SL=", sl);
   
   // Execute Order
   bool success = false;
   
   if(direction == "BUY") {
      success = g_trade.Buy(
         0.10,          // Volume (lot_size จาก strategy ถ้าต้องการก็ parse เพิ่ม)
         Symbol(),      // Symbol
         0,             // Price = 0 ใช้ Market Price
         sl,            // Stop Loss
         tp,            // Take Profit
         StringFormat("AGV-BRIDGE")
      );
   } else if(direction == "SELL") {
      success = g_trade.Sell(
         0.10,
         Symbol(),
         0,
         sl,
         tp,
         StringFormat("AGV-BRIDGE")
      );
   }
   
   if(success) {
      Print("✅ Order executed | Ticket: ", g_trade.ResultOrder());
      // แจ้ง aitrade ว่า MT5 confirm แล้ว (optional)
      NotifyOrderConfirm(g_trade.ResultOrder());
   } else {
      Print("❌ Order failed | Error: ", GetLastError(), " | Retcode: ", g_trade.ResultRetcode());
   }
}

//+------------------------------------------------------------------+
//| แจ้ง aitrade ว่า MT5 Confirm Order แล้ว (optional)               |
//+------------------------------------------------------------------+
void NotifyOrderConfirm(ulong ticket) {
   string payload = StringFormat(
      "{\"mt5_ticket\":%llu,\"status\":\"CONFIRMED\"}",
      ticket
   );
   // TODO: POST ไปที่ /api/mt5/confirm endpoint (ถ้าต้องการ implement)
   // PostToAitrade("/api/mt5/confirm", payload);
}

//+------------------------------------------------------------------+
//| Helper: Extract JSON string value                                 |
//+------------------------------------------------------------------+
string ExtractJsonString(const string json, const string key) {
   string search = "\"" + key + "\":\"";
   int start = StringFind(json, search);
   if(start == -1) return "";
   start += StringLen(search);
   int end = StringFind(json, "\"", start);
   if(end == -1) return "";
   return StringSubstr(json, start, end - start);
}

//+------------------------------------------------------------------+
//| Helper: Extract JSON double value                                 |
//+------------------------------------------------------------------+
double ExtractJsonDouble(const string json, const string key) {
   string search = "\"" + key + "\":";
   int start = StringFind(json, search);
   if(start == -1) return 0.0;
   start += StringLen(search);
   // หาจนถึง , หรือ }
   int end = start;
   while(end < StringLen(json) && 
         StringGetCharacter(json, end) != ',' && 
         StringGetCharacter(json, end) != '}') end++;
   return StringToDouble(StringSubstr(json, start, end - start));
}
//+------------------------------------------------------------------+
