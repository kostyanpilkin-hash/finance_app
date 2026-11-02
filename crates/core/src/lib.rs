//! Pure domain logic for Finance (no I/O). Shared by desktop, LAN host and the future server.

pub mod money;

pub use money::{Currency, Money, MoneyError};
