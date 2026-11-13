//! The only place that touches the OS keychain.
//!
//! All secrets live in a single `secrets` keychain entry — a JSON map of
//! `{account: hex-secret}` — and are cached in memory for the process run.
//! macOS prompts once per entry access, so one merged entry means at most
//! one password dialog per launch instead of one per background-loop tick.

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

const SERVICE: &str = "by.kostyan.finance";
const MERGED: &str = "secrets";
pub const DB_KEY: &str = "db-primary";
pub const DEVICE_TOKEN: &str = "device-token";
pub const BOT_TOKEN: &str = "bot-token";
const LEGACY: &[&str] = &[DB_KEY, DEVICE_TOKEN, BOT_TOKEN];

// Serialization — pure functions, covered by tests.
fn pack(map: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let obj: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                serde_json::json!(v.iter().map(|b| format!("{b:02x}")).collect::<String>()),
            )
        })
        .collect();
    serde_json::to_vec(&serde_json::Value::Object(obj)).unwrap_or_default()
}

fn unpack(b: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) else {
        return BTreeMap::new();
    };
    let mut m = BTreeMap::new();
    if let Some(o) = v.as_object() {
        for (k, hexs) in o {
            if let Some(h) = hexs.as_str() {
                if let Some(bytes) = decode_hex(h) {
                    m.insert(k.clone(), bytes);
                }
            }
        }
    }
    m
}

fn decode_hex(h: &str) -> Option<Vec<u8>> {
    if !h.len().is_multiple_of(2) {
        return None;
    }
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).ok())
        .collect()
}

fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
}

fn os_read(account: &str) -> Result<Option<Vec<u8>>, String> {
    match entry(account)?.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// In-memory map: account → Option<secret>. None = read once, no entry exists.
static CACHE: OnceLock<RwLock<BTreeMap<String, Option<Vec<u8>>>>> = OnceLock::new();
fn cache() -> &'static RwLock<BTreeMap<String, Option<Vec<u8>>>> {
    CACHE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

// One-time load/migration: merged entry wins; legacy entries are folded in
// and deleted only after the merged write succeeds.
static LOADED: OnceLock<()> = OnceLock::new();
fn ensure_loaded() {
    LOADED.get_or_init(|| {
        let mut map = match os_read(MERGED) {
            Ok(Some(b)) => unpack(&b)
                .into_iter()
                .map(|(k, v)| (k, Some(v)))
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };
        // Migrate legacy entries — only for accounts missing from merged.
        let mut migrated = false;
        for acc in LEGACY {
            if map.contains_key(*acc) {
                continue;
            }
            if let Ok(Some(b)) = os_read(acc) {
                map.insert((*acc).into(), Some(b));
                migrated = true;
            }
        }
        if migrated {
            let packed: BTreeMap<String, Vec<u8>> = map
                .iter()
                .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
                .collect();
            if entry(MERGED)
                .and_then(|e| e.set_secret(&pack(&packed)).map_err(|e| e.to_string()))
                .is_ok()
            {
                for acc in LEGACY {
                    if let Ok(e) = entry(acc) {
                        let _ = e.delete_credential();
                    }
                }
            }
        }
        *cache().write().unwrap_or_else(|e| e.into_inner()) = map;
    });
}

fn persist() {
    let snapshot: BTreeMap<String, Vec<u8>> = cache()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
        .collect();
    if let Ok(e) = entry(MERGED) {
        let _ = e.set_secret(&pack(&snapshot));
    }
}

pub fn read(account: &str) -> Result<Option<Vec<u8>>, String> {
    ensure_loaded();
    Ok(cache()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(account)
        .cloned()
        .flatten())
}

pub fn read_string(account: &str) -> Option<String> {
    read(account)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn write(account: &str, secret: &[u8]) -> Result<(), String> {
    ensure_loaded();
    cache()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(account.into(), Some(secret.to_vec()));
    persist();
    Ok(())
}

pub fn delete(account: &str) {
    ensure_loaded();
    cache()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(account.into(), None);
    persist();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), vec![0xde, 0xad, 0xbe, 0xef]);
        m.insert("b".to_string(), b"tok".to_vec());
        let u = unpack(&pack(&m));
        assert_eq!(u, m);
    }

    #[test]
    fn unpack_garbage_is_empty() {
        assert!(unpack(b"not json").is_empty());
        assert!(unpack(b"{\"a\": \"zz\"}").is_empty());
    }
}
