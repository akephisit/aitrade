//! Domain models shared across the entire Antigravity system.

pub mod strategy;
pub mod tick;

pub use strategy::{ActiveStrategy, Direction};
pub use tick::TickData;
