//! HTTP API for Finance (pairing, push/pull sync, files). Implemented in stage 5.

/// Version of the host/client sync protocol; bumped on incompatible changes.
pub const API_VERSION: u32 = 1;

pub use finance_storage as storage;
