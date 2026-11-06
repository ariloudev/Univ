//! A release's rows compared, as they are written, with the row index of the release before
//! ([`super::index`]): what the release before left for the build, and each table's index set
//! in its writer's state.

use std::path::Path;

use univ_model::release::Scope;

use super::index::{self, Next, Previous};
use super::{D1Writer, database};
use crate::release::Kept;

/// What the release before left for this one, in the state directory: its numbering and
/// placement, and its row index, none before the first release; and where this release's row
/// index is to be written.
pub struct Before<'a> {
    pub kept: &'a Kept,
    pub rows: Option<&'a Path>,
    pub next_rows: &'a Path,
}

impl D1Writer<'_> {
    /// Writes only what release `number`'s rows that follow change from those `before` holds:
    /// a row kept unchanged is not written, and once every row is in, those the release no
    /// longer holds are retired. Every row is recorded in the next row index.
    ///
    /// # Errors
    ///
    /// Fails when the row index cannot be read or the next one begun, or when it holds rows of
    /// a database this release does not have.
    pub fn compare_with(&mut self, before: &Before<'_>, number: u32) -> Result<(), String> {
        let next = before.next_rows;
        index::fresh(next).map_err(|e| format!("{}: {e}", next.display()))?;
        if let Some(dir) = before.rows {
            let problem = |e: std::io::Error| format!("the row index in {}: {e}", dir.display());
            for held in Previous::databases(dir).map_err(problem)? {
                if super::slot(held) >= self.databases.len() {
                    return Err(format!(
                        "the release before holds rows in {held}, which this release has not"
                    ));
                }
            }
        }
        for (i, tables) in self.databases.iter_mut().enumerate() {
            let db = database(i);
            for state in tables
                .iter_mut()
                .filter(|t| t.table.scope == Scope::PerRelease)
            {
                let name = state.table.name;
                state.next = Some(Next::new(next, db, name));
                if let Some(dir) = before.rows {
                    state.previous = Previous::read(dir, db, name)
                        .map_err(|e| format!("the row index in {}: {e}", dir.display()))?;
                }
            }
        }
        self.comparing = Some(number);
        Ok(())
    }
}
