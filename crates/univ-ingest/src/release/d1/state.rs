//! One table's rows of one database as the writer takes them: their statements, estimate,
//! fingerprint and the checks of their keys and numbers.

use std::collections::HashSet;
use std::hash::Hash;

use univ_model::release::{Fingerprinter, SqlValue, Statements, TABLES, Table};

/// One table's rows as they are written.
pub(super) struct TableState {
    pub(super) table: Table,
    pub(super) statements: Statements,
    pub(super) part: String,
    pub(super) parts: usize,
    pub(super) rows: u64,
    pub(super) bytes: u64,
    pub(super) fingerprint: Fingerprinter,
    /// The places of the key's columns among the table's.
    pub(super) key: Vec<usize>,
    /// The rows of a table checked by its content, kept for their digest: a few hundred.
    pub(super) content: Option<Vec<Vec<SqlValue>>>,
    pub(super) keys: HashSet<u128>,
    pub(super) repeated: u64,
    pub(super) first_repeated: Option<String>,
    pub(super) not_finite: Option<String>,
}

impl TableState {
    pub(super) fn new(table: Table) -> Self {
        Self {
            table,
            statements: Statements::new(table),
            part: String::new(),
            parts: 0,
            rows: 0,
            bytes: 0,
            fingerprint: Fingerprinter::new(table),
            key: table.key.iter().filter_map(|c| table.position(c)).collect(),
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

/// A 128-bit hash of the key values of `row` at `places` ([`crate::hash::hash128`]), each
/// value hashed with its type: a key is told from another by it, and no text is made of it.
pub(super) fn key_hash(places: &[usize], row: &[SqlValue]) -> u128 {
    crate::hash::hash128(|h| {
        for &i in places {
            match row.get(i) {
                Some(SqlValue::Integer(n)) => (1_u8, n).hash(h),
                Some(SqlValue::Real(x)) => (2_u8, x.to_bits()).hash(h),
                Some(SqlValue::Text(t)) => (3_u8, t).hash(h),
                Some(SqlValue::Null) | None => 0_u8.hash(h),
            }
        }
    })
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
