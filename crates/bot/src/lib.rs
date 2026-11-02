//! Telegram bot for Finance. Implemented in stage 4.

/// How long a user can undo their last bot entry.
pub const UNDO_WINDOW_MINUTES: i64 = 10;

pub use finance_storage as storage;
