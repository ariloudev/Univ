//! A release's D1 rows as SQL files ready for `wrangler d1 execute --file`, with the estimate
//! of the storage they take.
//!
//! The files are `d1/0000-schema.sql`, the schema and its version, then
//! `d1/{nnnn}-{table}-{part}.sql` for each table that has rows, in the order of
//! `univ_model::release::TABLES`, each at most [`MAX_FILE_BYTES`]. Executed in path order they
//! load the release; each holds `INSERT OR IGNORE` statements only (D1 runs a file without
//! explicit transactions), so a file executed twice changes nothing. The build checks that each
//! table's primary keys are unique and every number finite, so `INSERT OR IGNORE` can only skip
//! a row written before, identical, and no NULL stands for a number.

use std::collections::BTreeMap;

use univ_model::release::manifest::{D1_CAP_BYTES, D1Estimate, TableEstimate};
use univ_model::release::{Row, SCHEMA, SCHEMA_VERSION, SqlValue, TABLES, keys};

/// The largest D1 file written, unless one statement alone is larger.
pub const MAX_FILE_BYTES: usize = 32 << 20;

/// The rows of each table of a release.
#[derive(Debug, Default)]
pub struct D1Rows {
    tables: BTreeMap<&'static str, Vec<Vec<SqlValue>>>,
}

impl D1Rows {
    /// Adds `rows` to their table.
    pub fn add<R: Row>(&mut self, rows: &[R]) {
        self.tables
            .entry(R::TABLE.name)
            .or_default()
            .extend(rows.iter().map(Row::values));
    }

    /// The rows of the table `name`.
    #[must_use]
    pub fn rows(&self, name: &str) -> usize {
        self.tables.get(name).map_or(0, Vec::len)
    }

    /// The values of each row of the table `name`.
    #[must_use]
    pub fn table(&self, name: &str) -> &[Vec<SqlValue>] {
        self.tables.get(name).map_or(&[], Vec::as_slice)
    }

    /// The estimated storage of every row, by table, against the 10 GB cap, with each table's
    /// fingerprint.
    #[must_use]
    pub fn estimate(&self) -> D1Estimate {
        let tables: Vec<TableEstimate> = TABLES
            .iter()
            .map(|t| {
                let rows = self.table(t.name);
                TableEstimate {
                    table: t.name.to_owned(),
                    rows: rows.len() as u64,
                    estimated_bytes: rows.iter().map(|r| t.estimated_bytes(r)).sum(),
                    fingerprint: t.fingerprint(rows),
                }
            })
            .collect();
        D1Estimate {
            cap_bytes: D1_CAP_BYTES,
            estimated_bytes: tables.iter().map(|t| t.estimated_bytes).sum(),
            tables,
        }
    }

    /// The SQL files, by path within the release directory.
    #[must_use]
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let schema = format!(
            "{SCHEMA}\nINSERT OR IGNORE INTO schema_version (version) VALUES ({SCHEMA_VERSION});\n"
        );
        let mut files = vec![(
            format!("{}/0000-schema.sql", keys::D1_DIR),
            schema.into_bytes(),
        )];
        for (n, table) in TABLES.iter().enumerate() {
            let Some(rows) = self.tables.get(table.name).filter(|r| !r.is_empty()) else {
                continue;
            };
            let mut parts: Vec<String> = vec![String::new()];
            for statement in table.inserts(rows) {
                let last = parts
                    .last_mut()
                    .filter(|p| p.is_empty() || p.len() + statement.len() <= MAX_FILE_BYTES);
                match last {
                    Some(part) => part.push_str(&statement),
                    None => parts.push(statement),
                }
            }
            for (p, text) in parts.into_iter().enumerate() {
                let path = format!(
                    "{}/{:04}-{}-{:03}.sql",
                    keys::D1_DIR,
                    n + 1,
                    table.name,
                    p + 1
                );
                files.push((path, text.into_bytes()));
            }
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use univ_model::release::rows::{IdsIssuedRow, ReleaseRow};

    #[test]
    fn the_files_load_the_schema_then_each_table_in_order() {
        let mut rows = D1Rows::default();
        rows.add(&[IdsIssuedRow {
            release: 1,
            first: 1,
            last: 9,
        }]);
        rows.add::<ReleaseRow>(&[]);
        let files = rows.files();
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["d1/0000-schema.sql", "d1/0010-ids_issued-001.sql"]);
        let text = String::from_utf8(files[1].1.clone()).unwrap();
        assert_eq!(
            text,
            "INSERT OR IGNORE INTO ids_issued (release, first, last) VALUES\n(1, 1, 9);\n"
        );
        let estimate = rows.estimate();
        assert_eq!(estimate.tables.len(), TABLES.len());
        assert!(estimate.estimated_bytes > 0 && estimate.estimated_bytes < 100);
        assert_eq!(rows.rows("ids_issued"), 1);
    }
}
