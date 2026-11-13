//! Database-key provisioning. The host app injects a key source (OS keychain);
//! tests inject in-memory keys.

use std::sync::OnceLock;

use crate::error::Error;

/// Returns the stored DB key or None when nothing is stored yet.
type KeyReader = Box<dyn Fn(&str) -> Result<Option<Vec<u8>>, String> + Send + Sync>;
/// Persists the DB key.
type KeyWriter = Box<dyn Fn(&str, &[u8]) -> Result<(), String> + Send + Sync>;

static KEY_READER: OnceLock<KeyReader> = OnceLock::new();
static KEY_WRITER: OnceLock<KeyWriter> = OnceLock::new();

/// Called once by the desktop app (keychain) — or by tests (memory).
pub fn install_key_store(
    read: impl Fn(&str) -> Result<Option<Vec<u8>>, String> + Send + Sync + 'static,
    write: impl Fn(&str, &[u8]) -> Result<(), String> + Send + Sync + 'static,
) {
    let _ = KEY_READER.set(Box::new(read));
    let _ = KEY_WRITER.set(Box::new(write));
}

pub fn read_key(instance_id: &str) -> Result<Option<Vec<u8>>, Error> {
    match KEY_READER.get() {
        Some(f) => f(instance_id).map_err(Error::Other),
        None => Ok(None),
    }
}

pub fn write_key(instance_id: &str, key: &[u8]) -> Result<(), Error> {
    match KEY_WRITER.get() {
        Some(f) => f(instance_id, key).map_err(Error::Other),
        None => Err(Error::Other("no key store installed".into())),
    }
}

/// Uniform decimal code from the OS CSPRNG (rejection sampling: bytes
/// >= 250 are dropped so every digit has the same probability).
pub fn numeric_code(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        for b in uuid::Uuid::new_v4().into_bytes() {
            if b < 250 && out.len() < len {
                out.push(char::from(b'0' + b % 10));
            }
        }
    }
    out
}

/// 32-byte random key (uuid v4 draws from the OS CSPRNG).
pub fn generate_key() -> Vec<u8> {
    let mut k = Vec::with_capacity(32);
    k.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    k.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    k
}

/// Human-form recovery key: base32 groups, e.g. "K3F4-9XM2-..." (52 chars raw).
pub fn encode_recovery(key: &[u8]) -> String {
    let raw = data_encoding::BASE32_NOPAD.encode(key);
    raw.chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn decode_recovery(s: &str) -> Result<Vec<u8>, Error> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    data_encoding::BASE32_NOPAD
        .decode(cleaned.to_uppercase().as_bytes())
        .map_err(|e| Error::Invalid(format!("bad recovery key: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_code_len_and_digits() {
        for _ in 0..50 {
            let c = numeric_code(6);
            assert_eq!(c.len(), 6);
            assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        }
    }
}
