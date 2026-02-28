//! Domain models shared across the entire Antigravity system.

pub mod position;
pub mod strategy;
pub mod tick;

#[allow(unused_imports)]
pub use position::{OpenPosition, TradeRecord, TradeStatus};
pub use strategy::{ActiveStrategy, Direction};
pub use tick::TickData;
