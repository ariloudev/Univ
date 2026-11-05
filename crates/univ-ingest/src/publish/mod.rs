//! Publishing a complete release to the local D1 and R2 that `wrangler dev` reads
//! (publish-a-release-locally), and pruning one (prune-old-releases).
//!
//! Both work through a [`Store`], the D1 and R2 operations they need. [`Wrangler`] runs the D1
//! ones with Wrangler's `--local` commands against a `--persist-to` state directory, and every
//! R2 one of a run through one [`R2Batch`], a long-running script that opens the same local
//! bucket as Wrangler does; tests run the same logic against an in-memory store.
//!
//! # Publishing
//!
//! [`publish`] reads release n's manifest from its build directory, and refuses the release
//! when the state directory does not record it (its build stopped before it did), when it was
//! built before object shards, when it needs more object shards than Univ Server binds, or when
//! its D1 estimate in any database and those of every release not pruned together pass the CON-3
//! cap. It then uploads in this order, each unit checked before it is entered in the journal:
//!
//! 1. each database's schema, `d1/{dir}/0000-schema.sql`, the catalogue's first, checked by the
//!    schema version it records;
//! 2. each R2 file under `releases/{n}/`, read back and checked against its SHA-256 (AC-102);
//! 3. each table's D1 files in each database, the catalogue's first, checked by the count and
//!    fingerprint of the rows of release n the table then holds there against the manifest's
//!    (`univ_model::release::fingerprint`), so a row dropped or altered on its way to D1 stops
//!    the publish naming the database, the table and the measure;
//! 4. `manifest.yaml`, read back and checked against the local file;
//! 5. `latest`, moved to n by one single-statement write (AC-104), and only then the release's
//!    `published_at`.
//!
//! Until step 5 nothing a read naming no release selects has changed (AC-103): release n's rows
//! all carry `release = n`, which no read of the release `latest` names selects, and its files
//! lie under `releases/{n}/`, which no other release's keys share.
//!
//! # The journal
//!
//! Every unit uploaded and checked is entered in `publish-{n}.yaml` in the journal directory,
//! [`journal::Journal`], with the manifest's SHA-256. An interrupted publish leaves `latest`
//! where it was; the next run reads the journal and uploads only what it does not list
//! (AC-105). A journal of another manifest under the same number is refused. Each local file
//! is checked against its SHA-256 once, as its unit is uploaded.
//!
//! A run stopped after `latest` moved but before it recorded `published_at` is finished by the
//! next run of `publish n`, which records it whenever every unit of n is journaled, whatever
//! `latest` points to by then: the time `latest` moved, as `latest` or the journal gives it, or
//! else the time its last unit was checked.

mod check;
mod error;
pub mod journal;
mod progress;
mod prune;
mod r2batch;
mod refuse;
mod upload;
mod wrangler;

use std::fmt;
use std::path::Path;

use serde_json::{Map, Value};
use univ_model::release::shards::Database;

use crate::tiles::sha256;

pub use check::{Status, published_status, sql_text};
pub use error::PublishError;
pub use progress::{FileDone, Lines, Progress};
pub use prune::{PruneError, Pruned, prune, prune_with};
pub use r2batch::{R2Batch, SCRIPT};
pub use upload::{Published, move_latest, publish, publish_with};
pub use wrangler::{BUCKET, STDERR_LINES, Wrangler, ingest_dir, strip_ansi};

/// Where publishing and pruning find a release and keep their journals, and their clock.
pub struct StoreOptions<'a> {
    /// The build output, whose `releases/{n}` holds each release: what publishing uploads, and
    /// the manifest naming a release's R2 files when R2 no longer holds it.
    pub out: &'a Path,
    /// The state directory, which records each release built: publishing refuses one it does
    /// not record.
    pub state: &'a Path,
    /// The directory of the publish journals.
    pub journals: &'a Path,
    /// The time now, RFC 3339 in UTC.
    pub now: &'a dyn Fn() -> String,
}

/// A row a D1 query returns, each column by name.
pub type QueryRow = Map<String, Value>;

/// What R2 gave back of an object read back once it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadBack {
    /// Its size in bytes.
    pub bytes: u64,
    /// Its SHA-256, in lower-case hexadecimal.
    pub sha256: String,
}

/// The D1 and R2 operations publishing and pruning need. D1 is several databases: the
/// catalogue database and the object shards (`univ_model::release::shards`).
pub trait Store {
    /// Executes the SQL file `file` against the D1 database `database`.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when D1 refuses it.
    fn execute_file(&mut self, database: Database, file: &Path) -> Result<(), String>;

    /// Runs one SQL statement against the D1 database `database` and gives the rows it
    /// returns.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when D1 refuses it.
    fn query(&mut self, database: Database, sql: &str) -> Result<Vec<QueryRow>, String>;

    /// How many object shards Univ Server binds: shards `0` to this, less one.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when the bindings cannot be read.
    fn bound_shards(&mut self) -> Result<u32, String>;

    /// Writes the file `file` to R2 at `key`.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when R2 refuses it.
    fn put(&mut self, key: &str, file: &Path) -> Result<(), String>;

    /// Writes the file `file` to R2 at `key` and reads it back, giving the size and SHA-256 of
    /// what R2 gives back: none when R2 holds nothing at `key` once it was written.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when R2 refuses the write or the read.
    fn put_read_back(&mut self, key: &str, file: &Path) -> Result<Option<ReadBack>, String> {
        self.put(key, file)?;
        Ok(self.get(key)?.map(|back| ReadBack {
            bytes: back.len() as u64,
            sha256: sha256(&back),
        }))
    }

    /// Reads the object at `key` from R2: none where there is none.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when R2 cannot be read.
    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, String>;

    /// Deletes the object at `key` from R2, if there is one.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when R2 refuses it.
    fn delete(&mut self, key: &str) -> Result<(), String>;

    /// Deletes the objects at `keys` from R2, those there are.
    ///
    /// # Errors
    ///
    /// Fails, naming the object and the cause, when R2 refuses one.
    fn delete_keys(&mut self, keys: &[String]) -> Result<(), String> {
        keys.iter().try_for_each(|key| self.delete(key))
    }
}

/// Where the release `latest` points to was when an operation started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Latest(pub Option<u32>);

impl fmt::Display for Latest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(n) => write!(f, "release {n}"),
            None => f.write_str("no release"),
        }
    }
}
