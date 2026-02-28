//! # engine::candle_builder
//! 
//! สร้างแท่งเทียน (Candle) จาก Tick Data เพื่อนำไปใช้วิเคราะห์ Price Action
//! เช่น การหาไส้เทียน (Wick Rejection) สไตล์ SMC ใน Timeframe เล็ก (M1, M5)

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub symbol: String,
    /// เวลาเริ่มต้นของแท่งเทียนนี้ (ปัดเศษนาที)
    pub start_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub tick_count: u32,
}

impl Candle {
    pub fn new(symbol: &str, time: DateTime<Utc>, price: f64) -> Self {
        // ปัดเศษลงเป็นนาทีให้ตรงกับแท่ง M1
        let start_time = time
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        Self {
            symbol: symbol.to_string(),
            start_time,
            open: price,
            high: price,
            low: price,
            close: price,
            tick_count: 1,
        }
    }

    pub fn update(&mut self, price: f64) {
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.tick_count += 1;
    }

    /// ตรวจว่าแท่งนี้จบด้วย "ไส้เทียนถูกตบกลับ" (Rejection Wick) หรือไม่?
    /// - `is_buy_signal`: สนใจไส้ล่าง (Lower Wick)? ถ้าใช่ = แรงซื้อตบกลับ (Rejection Buy)
    /// - `min_wick_ratio`: ไส้ต้องยาวกี่ % ของทั้งแท่ง (เช่น 0.6 = 60%)
    pub fn has_rejection_wick(&self, is_buy_signal: bool, min_wick_ratio: f64) -> bool {
        let total_range = self.high - self.low;
        if total_range == 0.0 {
            return false;
        }

        let body_top = self.open.max(self.close);
        let body_bottom = self.open.min(self.close);

        if is_buy_signal {
            // Rejection สลับแรงเทขาย → หางล่าง (Lower Wick) ต้องยาวมาก
            let lower_wick = body_bottom - self.low;
            let wick_ratio = lower_wick / total_range;

            // ไส้ล่างยาวเกินกำหนดแปลว่ากวาดสภาพคล่องแล้วเด้งกลับ = Rejection!
            wick_ratio >= min_wick_ratio && self.close >= self.open // ยิ่งปิดเขียวยิ่งดี
        } else {
            // Rejection สลับแรงซื้อ → หางบน (Upper Wick) ต้องยาวมาก
            let upper_wick = self.high - body_top;
            let wick_ratio = upper_wick / total_range;

            wick_ratio >= min_wick_ratio && self.close <= self.open // ยิ่งปิดแดงยิ่งดี
        }
    }
}
