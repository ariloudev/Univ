//! One table's rows of one database as the writer takes them: their statements, estimate,
//! fingerprint and the checks of their keys and numbers.

use std::collections::HashSet;

use univ_model::release::{Fingerprinter, Scope, SqlValue, Statements, TABLES, Table};

use super::index::{Next, Previous, places};
use super::parts::Parts;

/// One table's rows as they are written.
pub(super) struct TableState {
    pub(super) table: Table,
    pub(super) statements: Statements,
    /// The files of its rows, and the next to be put.
    pub(super) parts: Parts,
    pub(super) rows: u64,
    pub(super) bytes: u64,
    pub(super) fingerprint: Fingerprinter,
    /// The places of the columns of its key, without `release` for a table whose rows releases
    /// share: unique among the rows of one release.
    pub(super) key: Vec<usize>,
    /// The places of every column but `release`, which a row's content is hashed by.
    pub(super) content_at: Vec<usize>,
    /// The place of `release`, for a table whose rows releases share.
    pub(super) release_at: Option<usize>,
    /// The index of the release before's rows, and the next release's, where rows are
    /// compared.
    pub(super) previous: Option<Previous>,
    pub(super) next: Option<Next>,
    /// The rows written, new or changed.
    pub(super) added: u64,
    /// The rows of the release before retired.
    pub(super) retired: u64,
    /// The rows of a table checked by its content, kept for their digest: a few hundred.
    pub(super) content: Option<Vec<Vec<SqlValue>>>,
    pub(super) keys: HashSet<u128>,
    pub(super) repeated: u64,
    pub(super) first_repeated: Option<String>,
    pub(super) not_finite: Option<String>,
}

impl TableState {
    pub(super) fn new(table: Table) -> Self {
        let shared = table.scope == Scope::PerRelease;
        let (key, content_at) = places(&table, shared);
        Self {
            table,
            statements: Statements::new(table),
            parts: Parts::default(),
            rows: 0,
            bytes: 0,
            fingerprint: Fingerprinter::new(table),
            key,
            content_at,
            release_at: table.position("release").filter(|_| shared),
            previous: None,
            next: None,
            added: 0,
            retired: 0,
            content: table.checked_by_content().then(Vec::new),
            keys: HashSet::new(),
            repeated: 0,
            first_repeated: None,
            not_finite: None,
        }
    }
}

/// A row's primary key, as its columns and SQL literals.
pub(super) fn key_of(table: &Table, row: &[SqlValue]) -> String {
    let parts: Vec<String> = table
        .key
        .iter()
        .filter_map(|c| {
            let v = table.position(c).and_then(|i| row.get(i))?;
            Some(format!("{c} = {}", v.literal()))
        })
        .collect();
    format!("({})", parts.join(", "))
}

/// The place in `TABLES` of the table named `name`, worked out when compiled for a row type.
pub(super) const fn place_of(name: &str) -> usize {
    let mut i = 0;
    while i < TABLES.len() {
        if same(TABLES[i].name.as_bytes(), name.as_bytes()) {
            return i;
        }
        i += 1;
    }
    TABLES.len()
}

const fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
