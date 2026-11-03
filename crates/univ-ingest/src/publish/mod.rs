//! Publishing a complete release to the local D1 and R2 that `wrangler dev` reads
//! (publish-a-release-locally), and pruning one (prune-old-releases).
//!
//! Both work through a [`Store`], the five D1 and R2 operations they need. [`Wrangler`] runs
//! them with Wrangler's `--local` commands against a `--persist-to` state directory; tests run
//! the same logic against an in-memory store.
//!
//! # Publishing
//!
//! [`publish`] reads release n's manifest from its build directory and checks every local file
//! against it, then uploads in this order, each unit checked before it is entered in the
//! journal:
//!
//! 1. `d1/0000-schema.sql`, checked by the schema version it records;
//! 2. each R2 file under `releases/{n}/`, read back and checked against its SHA-256 (AC-102);
//! 3. each table's D1 files, checked by the rows of release n the table then holds against the
//!    manifest's count of them;
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
//! (AC-105). A journal of another manifest under the same number is refused.

mod check;
pub mod journal;
mod prune;
mod upload;
mod wrangler;

use std::fmt;
use std::path::Path;

use serde_json::{Map, Value};

pub use check::{Status, published_status, sql_text};
pub use prune::{PruneError, PruneOptions, Pruned, prune};
pub use upload::{PublishError, PublishOptions, Published, move_latest, publish};
pub use wrangler::{BUCKET, DATABASE, STDERR_LINES, Wrangler, ingest_dir, strip_ansi};

/// A row a D1 query returns, each column by name.
pub type QueryRow = Map<String, Value>;

/// The D1 and R2 operations publishing and pruning need.
pub trait Store {
    /// Executes the SQL file `file` against D1.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when D1 refuses it.
    fn execute_file(&mut self, file: &Path) -> Result<(), String>;

    /// Runs one SQL statement against D1 and gives the rows it returns.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when D1 refuses it.
    fn query(&mut self, sql: &str) -> Result<Vec<QueryRow>, String>;

    /// Writes the file `file` to R2 at `key`.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when R2 refuses it.
    fn put(&mut self, key: &str, file: &Path) -> Result<(), String>;

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
