//! A file of a release's rows written straight into the local D1 database that `wrangler dev`
//! reads (publish-a-release-locally): each file in one transaction, and the database's
//! write-ahead log emptied once it is written. Wrangler's own `d1 execute` leaves the log as it
//! found it, so a publish of many files grew it without end and every run read it again.
//!
//! Miniflare keeps each database in `v3/d1/miniflare-D1DatabaseObject/<id>.sqlite`, its `<id>`
//! the Durable Object id workerd derives from the database's id with `idFromName`: the first 16
//! bytes of an HMAC-SHA-256 of the name, keyed by the SHA-256 of the object's unique key, then
//! the first 16 of an HMAC of those under the same key.
//!
//! No Worker may have the local D1 open while a publish writes it, as during a migration.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use univ_model::release::LOCAL_D1_FILES;
use univ_model::release::shards::Database;

/// The unique key Miniflare gives the Durable Object class of D1.
const UNIQUE_KEY: &str = "miniflare-D1DatabaseObject";

/// SHA-256's block size, in bytes.
const BLOCK: usize = 64;

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// The name of the file Miniflare keeps the local D1 database of id `database_id` in, without
/// its directory.
#[must_use]
pub fn file_name(database_id: &str) -> String {
    let key: [u8; 32] = Sha256::digest(UNIQUE_KEY.as_bytes()).into();
    let base = &hmac_sha256(&key, database_id.as_bytes())[..16];
    let mac = &hmac_sha256(&key, base)[..16];
    format!("{}{}.sqlite", hex(base), hex(mac))
}

/// `bytes` in lower-case hexadecimal.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The id that `toml`, a Wrangler configuration, gives the D1 database `database` by its name.
#[must_use]
pub fn database_id(toml: &str, database: Database) -> Option<String> {
    let name = format!("database_name = \"{}\"", database.name());
    toml.split("[[d1_databases]]")
        .skip(1)
        .map(|block| block.split("[[").next().unwrap_or(block))
        .find(|block| block.lines().any(|l| l.trim() == name))?
        .lines()
        .find_map(|l| {
            let (field, value) = l.split_once('=')?;
            (field.trim() == "database_id").then(|| value.trim().trim_matches('"').to_owned())
        })
}

/// The file of the local D1 database of id `database_id` in Wrangler's state directory
/// `persist_to`.
#[must_use]
pub fn database_file(persist_to: &Path, database_id: &str) -> PathBuf {
    persist_to.join(LOCAL_D1_FILES).join(file_name(database_id))
}

/// Writes the SQL file `file` into the `SQLite` database `db` in one transaction, with foreign
/// keys enforced as D1 enforces them, then empties the database's write-ahead log.
///
/// # Errors
///
/// Fails, naming the files, when a statement fails, which leaves none of the file's rows, or
/// when the log cannot be emptied, as when a Worker has the database open.
pub fn write(db: &Path, file: &Path) -> Result<(), String> {
    let at = |e: &dyn std::fmt::Display| format!("{} into {}: {e}", file.display(), db.display());
    let sql = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut conn = Connection::open(db).map_err(|e| at(&e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| at(&e))?;
    let tx = conn.transaction().map_err(|e| at(&e))?;
    tx.execute_batch(&sql).map_err(|e| at(&e))?;
    tx.commit().map_err(|e| at(&e))?;
    let busy: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(|e| at(&e))?;
    if busy != 0 {
        return Err(at(
            &"its rows are written, but its write-ahead log could not be emptied: \
              is a Worker reading the local D1?",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}
