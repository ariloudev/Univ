//! A release's rows compared, as they are written, with the row index of the release before
//! ([`super::index`]): which are written and which kept, and the statements that retire the
//! rows the release no longer holds.

use std::path::{Path, PathBuf};

use univ_model::release::shards::Database;
use univ_model::release::sql::MAX_STATEMENT_BYTES;
use univ_model::release::{RETIRED, Scope, SqlValue, TABLES, Table};

use super::index::{Compared, Indexed, Next, Previous, hash_at, tuple_at};
use crate::release::Kept;

/// The comparison of release `number`'s rows with the release before's.
pub struct Comparing {
    pub(super) number: u32,
    pub(super) previous: Previous,
    pub(super) next: Next,
}

/// What the release before left for this one, in the state directory: its numbering and
/// placement, and its row index, none before the first release; and where this release's row
/// index is to be written.
pub struct Before<'a> {
    pub kept: &'a Kept,
    pub rows: Option<&'a Path>,
    pub next_rows: &'a Path,
}

impl Comparing {
    /// The comparison of release `number`'s rows with those `before` holds.
    ///
    /// # Errors
    ///
    /// Fails when the row index cannot be read or the next one begun.
    pub fn of(before: &Before<'_>, number: u32) -> Result<Self, String> {
        let tables: Vec<&'static Table> = TABLES
            .iter()
            .filter(|t| t.scope == Scope::PerRelease)
            .collect();
        let previous = match before.rows {
            Some(dir) => Previous::read(dir, &tables)
                .map_err(|e| format!("the row index in {}: {e}", dir.display()))?,
            None => Previous::default(),
        };
        let next = Next::new(before.next_rows)
            .map_err(|e| format!("{}: {e}", before.next_rows.display()))?;
        Ok(Self::new(number, previous, next))
    }

    /// Compares release `number`'s rows with `previous`, recording them in `next`.
    #[must_use]
    pub fn new(number: u32, previous: Previous, next: Next) -> Self {
        Self {
            number,
            previous,
            next,
        }
    }

    /// Compares a row of `table` in `database`, its key and content at `natural` and `content`
    /// among its values: the row as the release holds it when kept, with the release that added
    /// it, or none when it is to be written. Either way it is recorded for the next release.
    pub(super) fn row(
        &mut self,
        database: Database,
        table: &Table,
        (natural, content): (&[usize], &[usize]),
        values: &[SqlValue],
    ) -> Result<Option<Vec<SqlValue>>, String> {
        let key = hash_at(natural, values);
        let hashed = hash_at(content, values);
        let compared = self.previous.compare(database, table.name, key, hashed);
        let (kept, release) = match compared {
            Compared::Keep { release } => {
                let mut row = values.to_vec();
                if let Some(at) = table.position("release") {
                    row[at] = SqlValue::Integer(i64::from(release));
                }
                (Some(row), release)
            }
            Compared::Write => (None, self.number),
        };
        let indexed = Indexed {
            key,
            content: hashed,
            release,
        };
        self.next
            .record(database, table.name, indexed, &tuple_at(natural, values))
            .map_err(|e| format!("the row index of {}: {e}", table.name))?;
        Ok(kept)
    }

    /// The statements that retire the rows of `table` in `database` the release no longer
    /// holds, by their key, each within [`MAX_STATEMENT_BYTES`], and how many rows they retire.
    pub(super) fn retirements(
        &self,
        database: Database,
        table: &Table,
    ) -> Result<(Vec<String>, u64), String> {
        let keys = self
            .previous
            .retired(database, table.name)
            .map_err(|e| format!("the row index of {}: {e}", table.name))?;
        let columns: Vec<&str> = table
            .key
            .iter()
            .copied()
            .filter(|c| *c != "release")
            .collect();
        let n = self.number;
        let head = format!(
            "UPDATE {} SET {RETIRED} = {n} WHERE {RETIRED} IS NULL AND release < {n} AND ({}) \
             IN (VALUES ",
            table.name,
            columns.join(", ")
        );
        let mut statements = Vec::new();
        let mut current = String::new();
        for key in &keys {
            if !current.is_empty() && current.len() + key.len() + 4 > MAX_STATEMENT_BYTES {
                statements.push(format!("{current});\n"));
                current.clear();
            }
            if current.is_empty() {
                current.push_str(&head);
            } else {
                current.push_str(", ");
            }
            current.push_str(key);
        }
        if !current.is_empty() {
            statements.push(format!("{current});\n"));
        }
        Ok((statements, keys.len() as u64))
    }

    /// The databases whose rows the release before held.
    #[must_use]
    pub(super) fn databases(&self) -> Vec<Database> {
        self.previous.databases()
    }

    /// Writes out the next release's row index, giving its directory.
    pub(super) fn finish(self) -> Result<PathBuf, String> {
        self.next
            .finish()
            .map_err(|e| format!("the row index cannot be written: {e}"))
    }
}
