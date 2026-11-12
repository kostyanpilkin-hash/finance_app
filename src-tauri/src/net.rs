//! One process-wide `reqwest::Client` shared by sync, the bot and NBRB rate
//! fetches — connection pooling and DNS cache are reused instead of building a
//! client per call.
//!
//! No global request timeout on purpose: the bot's 25 s long-poll and quick
//! LAN calls need different budgets, so callers set per-request timeouts.
//! `connect_timeout` still bounds a dead host's handshake.

use std::sync::OnceLock;
use std::time::Duration;

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

pub fn client() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default()
    })
}
