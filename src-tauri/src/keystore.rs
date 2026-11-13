//! The only place that touches the OS keychain.

const SERVICE: &str = "by.kostyan.finance";
pub const DB_KEY: &str = "db-primary";
pub const DEVICE_TOKEN: &str = "device-token";
pub const BOT_TOKEN: &str = "bot-token";

fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
}

pub fn read(account: &str) -> Result<Option<Vec<u8>>, String> {
    match entry(account)?.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn read_string(account: &str) -> Option<String> {
    read(account)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn write(account: &str, secret: &[u8]) -> Result<(), String> {
    entry(account)?
        .set_secret(secret)
        .map_err(|e| e.to_string())
}

pub fn delete(account: &str) {
    if let Ok(e) = entry(account) {
        let _ = e.delete_credential();
    }
}
