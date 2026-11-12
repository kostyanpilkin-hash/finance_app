//! Staged backup restore: a chosen backup is validated and copied next to the
//! live database as `finance.db.restore`, then swapped in at the next startup
//! before any connection exists. The previous live db is kept as a
//! `pre-restore-*` copy — nothing is ever overwritten irreversibly.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::error::Error;

pub const DB_FILE: &str = "finance.db";
pub const RESTORE_FILE: &str = "finance.db.restore";
pub const BACKUP_DIR: &str = "backups";

/// Backup file names in `<data_dir>/backups`, newest first by modified time.
/// Only "*.db" files — the -wal/-shm sidecars of a copied db are not backups.
pub fn list(data_dir: &Path) -> Vec<String> {
    let dir = data_dir.join(BACKUP_DIR);
    let mut v: Vec<(String, std::time::SystemTime)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".db"))
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        e.metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|f| std::cmp::Reverse(f.1));
    v.into_iter().map(|(n, _)| n).collect()
}

/// Validates `backups/<name>` (decrypts with `key`, checks integrity) and
/// stages it as `finance.db.restore`. The live database is untouched.
pub fn stage_restore(data_dir: &Path, name: &str, key: &[u8]) -> Result<(), Error> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name.ends_with(".db")
    {
        return Err(Error::Invalid("bad backup name".into()));
    }
    let src = data_dir.join(BACKUP_DIR).join(name);
    if !src.is_file() {
        return Err(Error::NotFound(format!("backup {name}")));
    }
    // Open the source read-write exactly like Db::init_conn: the key pragma,
    // a probe query, a checkpoint (folds a pre-restore copy's -wal into its
    // .db; no-op for VACUUM INTO backups), then an integrity check.
    {
        let conn = Connection::open(&src)?;
        conn.pragma_update(None, "key", crate::hex(key))?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::Invalid(format!("backup integrity check: {check}")));
        }
    }
    let tmp = data_dir.join(format!("{RESTORE_FILE}.tmp"));
    let staged = data_dir.join(RESTORE_FILE);
    std::fs::copy(&src, &tmp)?;
    // sync needs a write-capable handle on Windows.
    std::fs::OpenOptions::new()
        .write(true)
        .open(&tmp)?
        .sync_all()?;
    std::fs::rename(&tmp, &staged)?; // replaces an existing staged file
    Ok(())
}

/// Called at startup before any connection exists. When a staged restore is
/// pending, moves the live db aside into `backups/pre-restore-<ts>.db` (with
/// its -wal next to it) and installs the staged file. Returns the path of the
/// preserved pre-restore copy, or None when there was nothing staged / no db.
pub fn apply_pending_restore(data_dir: &Path) -> Result<Option<PathBuf>, Error> {
    let staged = data_dir.join(RESTORE_FILE);
    if !staged.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(data_dir.join(BACKUP_DIR))?;
    // Stamp comes from the staged file's mtime so an interrupted run retries
    // under the same name instead of duplicating the preserved copy.
    let stamp = {
        let mtime = staged.metadata()?.modified()?;
        let t: chrono::DateTime<chrono::Local> = mtime.into();
        t.format("%Y%m%d-%H%M%S").to_string()
    };
    let db = data_dir.join(DB_FILE);
    let mut preserved = None;
    if db.exists() {
        let dest = data_dir
            .join(BACKUP_DIR)
            .join(format!("pre-restore-{stamp}.db"));
        std::fs::rename(&db, &dest)?;
        preserved = Some(dest);
    }
    // The WAL moves even when the db is already gone — a leftover finance.db-wal
    // must never be replayed into the restored file.
    let wal = data_dir.join(format!("{DB_FILE}-wal"));
    if wal.exists() {
        std::fs::rename(
            &wal,
            data_dir
                .join(BACKUP_DIR)
                .join(format!("pre-restore-{stamp}.db-wal")),
        )?;
    }
    let _ = std::fs::remove_file(data_dir.join(format!("{DB_FILE}-shm")));
    let _ = std::fs::remove_file(data_dir.join(format!("{RESTORE_FILE}.tmp")));
    std::fs::rename(&staged, &db)?;
    Ok(preserved)
}
