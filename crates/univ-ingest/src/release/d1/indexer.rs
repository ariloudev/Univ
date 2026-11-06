//! A table's row index written from the rows a database holds of one release, rather than by a
//! build: what a reindex makes again of the release `latest` points to.

use std::path::Path;

use univ_model::release::shards::Database;
use univ_model::release::{Scope, SqlValue, Table};

use super::index::{Indexed, Next, hash_at, key_text, places};

/// A table's row index written from its rows as a database holds them, valid in one release:
/// each with the release that added it, as `release` gives it.
pub struct Indexer {
    next: Next,
    table: Table,
    key: Vec<usize>,
    content: Vec<usize>,
    release: usize,
    text: String,
}

impl Indexer {
    /// The index of `table` in `database`, written in `dir`.
    ///
    /// # Errors
    ///
    /// Fails for a table whose rows releases do not share.
    pub fn new(dir: &Path, database: Database, table: Table) -> Result<Self, String> {
        let release = table
            .position("release")
            .filter(|_| table.scope == Scope::PerRelease)
            .ok_or_else(|| format!("{} is not a table whose rows releases share", table.name))?;
        let (key, content) = places(&table, true);
        Ok(Self {
            next: Next::new(dir, database, table.name),
            table,
            key,
            content,
            release,
            text: String::new(),
        })
    }

    /// Records a row, its values in the order of the table's columns.
    ///
    /// # Errors
    ///
    /// Fails when its release is not a number, or the file cannot be written.
    pub fn record(&mut self, values: &[SqlValue]) -> Result<(), String> {
        let release = match values.get(self.release) {
            Some(SqlValue::Integer(n)) => u32::try_from(*n).ok(),
            _ => None,
        }
        .ok_or_else(|| format!("a row of {} has no release", self.table.name))?;
        let row = Indexed {
            key: hash_at(self.table.kinds, &self.key, values),
            content: hash_at(self.table.kinds, &self.content, values),
            release,
        };
        key_text(&self.key, values, &mut self.text);
        self.next.record(row, &self.text)
    }

    /// Writes out what is buffered.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn finish(mut self) -> Result<(), String> {
        self.next.flush()
    }
}
