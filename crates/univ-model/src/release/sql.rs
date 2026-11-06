//! The tables of the D1 schema as Rust sees them, SQL values and their literals, the `INSERT`
//! statements a release's rows become, and the estimate of how much D1 storage a row takes.
//!
//! # The size estimate
//!
//! A row is estimated as SQLite stores it (the record format of `sqlite.org/fileformat2.html`):
//! a header of one varint for its size and one serial-type varint per column, then each value
//! (0 bytes for a NULL or the integers 0 and 1, 1 to 8 for another integer, 8 for a real, the
//! UTF-8 length for a text), plus 4 bytes of cell pointer and payload-size varint in its B-tree
//! page. Each secondary index stores its columns and the primary key again, with the same
//! overhead. The bytes a cell keeps on its B-tree page are divided by [`PAGE_FILL`], the share
//! of a page that holds cells, so the estimate errs high rather than low.
//!
//! A record larger than its tree's local payload limit on a [`PAGE_SIZE`] page keeps only part
//! of itself in its cell and the rest in overflow pages, each a whole page however little of it
//! is used, as `sqlite.org/fileformat2.html` gives (§ 1.6, with U the usable page size, 4096,
//! and P the record's bytes):
//!
//! - the most a cell keeps is X = U − 35 in a table B-tree (a rowid table) and
//!   X = ((U − 12) × 64 / 255) − 23 in an index B-tree (an index, or a `WITHOUT ROWID` table);
//! - a record over X keeps K = M + ((P − M) mod (U − 4)) bytes, M = ((U − 12) × 32 / 255) − 23,
//!   when K ≤ X, else M, and a 4-byte pointer to the first overflow page;
//! - the rest fills overflow pages of U − 4 bytes each.
//!
//! So a record of 1.1 kB in a `WITHOUT ROWID` table keeps 489 bytes in its cell and takes one
//! whole 4096-byte overflow page for the remaining 611: about 4.7 kB of storage.

use std::fmt::Write as _;

/// The share of a B-tree page an estimate takes as holding cells: pages split by inserts are
/// often only two-thirds full.
pub const PAGE_FILL: f64 = 0.65;
/// The page size the estimate takes: SQLite's default, and D1's.
pub const PAGE_SIZE: u64 = 4096;
/// The usable bytes of a page: no bytes are reserved.
const USABLE: u64 = PAGE_SIZE;
/// The least a cell keeps of a record that overflows: M.
const MIN_LOCAL: u64 = (USABLE - 12) * 32 / 255 - 23;
/// The most a table B-tree cell keeps: X of a rowid table.
const MAX_LOCAL_TABLE: u64 = USABLE - 35;
/// The most an index B-tree cell keeps: X of an index or a `WITHOUT ROWID` table.
const MAX_LOCAL_INDEX: u64 = (USABLE - 12) * 64 / 255 - 23;
/// The longest statement written: D1 refuses an SQL statement over 100,000 bytes.
pub const MAX_STATEMENT_BYTES: usize = 90_000;

/// A value of one column.
#[derive(Clone, Debug, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

impl From<Option<f64>> for SqlValue {
    fn from(value: Option<f64>) -> Self {
        value.map_or(Self::Null, Self::Real)
    }
}

impl From<Option<String>> for SqlValue {
    fn from(value: Option<String>) -> Self {
        value.map_or(Self::Null, Self::Text)
    }
}

impl From<String> for SqlValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for SqlValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<f64> for SqlValue {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

impl From<i64> for SqlValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<bool> for SqlValue {
    fn from(value: bool) -> Self {
        Self::Integer(i64::from(value))
    }
}

/// Whether a text is written as a cast of its bytes: it holds a NUL or words Wrangler strips.
fn needs_hex(text: &str) -> bool {
    text.contains('\0')
        || contains_ignoring_case(text, b"BEGIN TRANSACTION")
        || contains_ignoring_case(text, b"COMMIT")
}

/// Whether `text` holds `upper`, an upper-case ASCII word, in any case.
fn contains_ignoring_case(text: &str, upper: &[u8]) -> bool {
    text.as_bytes()
        .windows(upper.len())
        .any(|w| w.eq_ignore_ascii_case(upper))
}

/// Bytes of a varint holding `n`.
fn varint_bytes(n: u64) -> u64 {
    match n {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x001f_ffff => 3,
        0x0020_0000..=0x0fff_ffff => 4,
        _ => 9,
    }
}

impl SqlValue {
    /// The value as an SQL literal: a text quoted with its quotes doubled, a real as the
    /// shortest decimal that reads back the same, a real that is not finite as NULL (which a
    /// build refuses before it writes a row).
    ///
    /// A text is written as a cast of its hexadecimal bytes instead when a quoted literal would
    /// not reach D1 as it is: one holding a NUL, which a literal cannot, and one holding
    /// `BEGIN TRANSACTION` or `COMMIT` in any case, which `wrangler d1 execute --file` strips
    /// from a file's SQL wherever they stand, inside quotes too (its `trimSqlQuery`).
    #[must_use]
    pub fn literal(&self) -> String {
        let mut out = String::new();
        self.write_literal(&mut out);
        out
    }

    /// Appends the value's [`SqlValue::literal`] to `out`, with no text of its own made.
    pub fn write_literal(&self, out: &mut String) {
        // Writing into a String cannot fail.
        match self {
            Self::Integer(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Real(x) if x.is_finite() => {
                let _ = write!(out, "{x:?}");
            }
            Self::Null | Self::Real(_) => out.push_str("NULL"),
            Self::Text(t) if needs_hex(t) => {
                out.push_str("CAST(X'");
                for b in t.bytes() {
                    let _ = write!(out, "{b:02X}");
                }
                out.push_str("' AS TEXT)");
            }
            Self::Text(t) => {
                out.push('\'');
                for (i, part) in t.split('\'').enumerate() {
                    if i > 0 {
                        out.push_str("''");
                    }
                    out.push_str(part);
                }
                out.push('\'');
            }
        }
    }

    /// Its serial type's bytes in a record header and its own bytes in the record body.
    fn stored(&self) -> (u64, u64) {
        match self {
            Self::Null | Self::Integer(0 | 1) => (1, 0),
            Self::Integer(n) => {
                let body = match n.unsigned_abs() {
                    0..=0x7f => 1,
                    0x80..=0x7fff => 2,
                    0x8000..=0x007f_ffff => 3,
                    0x0080_0000..=0x7fff_ffff => 4,
                    0x8000_0000..=0x7fff_ffff_ffff => 6,
                    _ => 8,
                };
                (1, body)
            }
            Self::Real(_) => (1, 8),
            Self::Text(t) => {
                let len = t.len() as u64;
                (varint_bytes(len * 2 + 13), len)
            }
        }
    }
}

/// The bytes of a record of `values`: its header and body.
fn payload_bytes<'a>(values: impl IntoIterator<Item = &'a SqlValue>) -> u64 {
    let (header, body) = values
        .into_iter()
        .map(SqlValue::stored)
        .fold((0, 0), |(h, b), (sh, sb)| (h + sh, b + sb));
    let header = header + varint_bytes(header + 1);
    header + body
}

/// The kind of B-tree a record is stored in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    /// A rowid table's.
    Table,
    /// An index's, or a `WITHOUT ROWID` table's.
    Index,
}

/// The bytes a record of `payload` bytes keeps in its cell, and the overflow pages it takes.
#[must_use]
pub fn split_payload(payload: u64, tree: Tree) -> (u64, u64) {
    let max_local = match tree {
        Tree::Table => MAX_LOCAL_TABLE,
        Tree::Index => MAX_LOCAL_INDEX,
    };
    if payload <= max_local {
        return (payload, 0);
    }
    let k = MIN_LOCAL + (payload - MIN_LOCAL) % (USABLE - 4);
    let local = if k <= max_local { k } else { MIN_LOCAL };
    (local, (payload - local).div_ceil(USABLE - 4))
}

/// The estimated storage of a record of `payload` bytes in a `tree`: its cell, with 4 bytes of
/// cell pointer and payload size (and 4 of overflow pointer, where it overflows), over
/// [`PAGE_FILL`], and its overflow pages whole.
fn stored_bytes(payload: u64, tree: Tree) -> f64 {
    let (local, overflow) = split_payload(payload, tree);
    let pointer = if overflow > 0 { 4 } else { 0 };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a row's bytes are far below 2^52"
    )]
    let bytes = (local + 4 + pointer) as f64 / PAGE_FILL + (overflow * PAGE_SIZE) as f64;
    bytes
}

/// The type of a column's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    Integer,
    Real,
    Text,
}

/// Which rows of a table belong to release n.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Rows shared by the releases they are valid in: each row is valid from the release in its
    /// `release` column, which ends its key, until the release in its `retired` column, if any,
    /// which a later release sets (see [`RETIRED`]).
    PerRelease,
    /// The id ledger, kept across releases and never pruned: release n's view of it is the rows
    /// whose `release` is at most n, which each release carries whole.
    Ledger,
    /// The list of releases: release n's row is the one numbered n, kept when it is pruned.
    Releases,
}

impl Scope {
    /// The SQL condition selecting release `number`'s rows.
    #[must_use]
    pub fn filter(self, number: u32) -> String {
        match self {
            Self::PerRelease => super::shared::valid_in("", number),
            Self::Ledger => format!("release <= {number}"),
            Self::Releases => format!("number = {number}"),
        }
    }
}

/// The `retired` a shared row is estimated with: a release number SQLite stores in two bytes.
const RETIRED_AT: SqlValue = SqlValue::Integer(1_000);

/// A table of the schema: its columns in the order rows give them with the type of each, its
/// primary key and its secondary indexes, each by column names, and which of its rows belong to
/// a release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Table {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub kinds: &'static [ColumnKind],
    pub key: &'static [&'static str],
    pub indexes: &'static [&'static [&'static str]],
    pub scope: Scope,
}

impl Table {
    /// The position of `column` among the columns.
    #[must_use]
    pub fn position(&self, column: &str) -> Option<usize> {
        self.columns.iter().position(|c| *c == column)
    }

    /// The B-tree its rows are stored in: a table whose key is one integer column is a rowid
    /// table (`INTEGER PRIMARY KEY`), every other table of the schema is `WITHOUT ROWID`.
    #[must_use]
    pub fn tree(&self) -> Tree {
        match self.key {
            [one] if self.position(one).map(|i| self.kinds[i]) == Some(ColumnKind::Integer) => {
                Tree::Table
            }
            _ => Tree::Index,
        }
    }

    /// The estimated D1 bytes of a row of `values`, in the order of [`Table::columns`]: the
    /// row's record and one per secondary index, each cell over [`PAGE_FILL`] and each overflow
    /// page whole.
    #[must_use]
    pub fn estimated_bytes(&self, values: &[SqlValue]) -> u64 {
        // A row shared between releases also stores its `retired`: NULL when written, a release
        // number once a later release retires it, counted at two bytes so the estimate errs high.
        let retired = (self.scope == Scope::PerRelease).then_some(&RETIRED_AT);
        let stored = values.iter().chain(retired);
        let mut bytes = stored_bytes(payload_bytes(stored), self.tree());
        for index in self.indexes {
            let columns = index.iter().chain(self.key.iter());
            let picked = columns.filter_map(|c| self.position(c).and_then(|i| values.get(i)));
            bytes += stored_bytes(payload_bytes(picked), Tree::Index);
        }
        // Rounded up: the estimate errs high.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a row's bytes are far below 2^52 and positive"
        )]
        let total = bytes.ceil() as u64;
        total
    }

    /// The head of an `INSERT OR IGNORE` statement into the table, naming its columns.
    #[must_use]
    pub fn insert_head(&self) -> String {
        format!(
            "INSERT OR IGNORE INTO {} ({}) VALUES\n",
            self.name,
            self.columns.join(", ")
        )
    }
}

/// Appends a row's values to `out` as an SQL tuple, `(1, 'a', NULL)`, each literal written in
/// place.
pub fn write_tuple(values: &[SqlValue], out: &mut String) {
    out.push('(');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        v.write_literal(out);
    }
    out.push(')');
}

/// A table's `INSERT OR IGNORE` statements made one row at a time: a statement is given once
/// the next row would take it over [`MAX_STATEMENT_BYTES`].
pub struct Statements {
    head: String,
    current: String,
    /// The row being added, as its tuple: one buffer for every row.
    tuple: String,
}

impl Statements {
    /// No statement yet, into `table`.
    #[must_use]
    pub fn new(table: Table) -> Self {
        Self {
            head: table.insert_head(),
            current: String::new(),
            tuple: String::new(),
        }
    }

    /// Adds a row, giving the statement it closed, if any.
    pub fn push(&mut self, row: &[SqlValue]) -> Option<String> {
        self.tuple.clear();
        write_tuple(row, &mut self.tuple);
        let tuple = &self.tuple;
        let mut closed = None;
        if !self.current.is_empty() && self.current.len() + tuple.len() + 3 > MAX_STATEMENT_BYTES {
            self.current.push_str(";\n");
            closed = Some(std::mem::take(&mut self.current));
        }
        if self.current.is_empty() {
            self.current.push_str(&self.head);
        } else {
            self.current.push_str(",\n");
        }
        self.current.push_str(tuple);
        closed
    }

    /// The last statement, if a row was added since one was given.
    pub fn finish(&mut self) -> Option<String> {
        if self.current.is_empty() {
            return None;
        }
        self.current.push_str(";\n");
        Some(std::mem::take(&mut self.current))
    }
}

/// A row of a table of the schema.
pub trait Row {
    /// The table it is a row of.
    const TABLE: Table;
    /// Its values in the order of the table's columns.
    fn values(&self) -> Vec<SqlValue>;
}

/// The statements [`Statements`] makes of `rows`, for a test.
#[cfg(test)]
pub(crate) fn inserts(table: Table, rows: &[Vec<SqlValue>]) -> Vec<String> {
    let mut statements = Statements::new(table);
    let mut out: Vec<String> = rows.iter().filter_map(|r| statements.push(r)).collect();
    out.extend(statements.finish());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_quote_text_and_keep_reals_exact() {
        assert_eq!(SqlValue::from("O'Brien").literal(), "'O''Brien'");
        assert_eq!(SqlValue::from("a\0b").literal(), "CAST(X'610062' AS TEXT)");
        assert_eq!(SqlValue::Real(0.1).literal(), "0.1");
        assert_eq!(SqlValue::Real(1e300).literal(), "1e300");
        assert_eq!(SqlValue::Real(2.0).literal(), "2.0");
        assert_eq!(SqlValue::Real(f64::NAN).literal(), "NULL");
        assert_eq!(SqlValue::Integer(-7).literal(), "-7");
        assert_eq!(SqlValue::from(None::<f64>).literal(), "NULL");
    }

    #[test]
    fn a_text_wrangler_would_strip_is_written_as_its_bytes() {
        assert_eq!(
            SqlValue::from("BEGIN TRANSACTION;").literal(),
            "CAST(X'424547494E205452414E53414354494F4E3B' AS TEXT)"
        );
        assert_eq!(
            SqlValue::from("a commit;").literal(),
            "CAST(X'6120636F6D6D69743B' AS TEXT)"
        );
        assert_eq!(
            SqlValue::from("committee").literal(),
            "CAST(X'636F6D6D6974746565' AS TEXT)"
        );
        assert_eq!(SqlValue::from("begin").literal(), "'begin'");
    }

    const T: Table = Table {
        name: "t",
        columns: &["release", "id", "name"],
        key: &["release", "id"],
        kinds: &[ColumnKind::Integer, ColumnKind::Integer, ColumnKind::Text],
        indexes: &[&["name"]],
        scope: Scope::PerRelease,
    };

    #[test]
    fn a_row_is_estimated_as_sqlite_stores_it_with_its_index() {
        let row = vec![
            SqlValue::Integer(1),
            SqlValue::Integer(300),
            SqlValue::from("abcd"),
        ];
        // Record: header 1 + 4 serial types (the last for `retired`, two bytes), body 0 + 2 +
        // 4 + 2, cell 4: 17. Index on (name, release, id): header 4, body 4 + 0 + 2, cell 4:
        // 14. 31 / 0.65 = 47.7, rounded up.
        assert_eq!(T.estimated_bytes(&row), 48);
    }

    #[test]
    fn inserts_split_before_the_statement_limit() {
        let long = "x".repeat(40_000);
        let rows: Vec<Vec<SqlValue>> = (0..5)
            .map(|i| {
                vec![
                    SqlValue::Integer(1),
                    SqlValue::Integer(i),
                    long.as_str().into(),
                ]
            })
            .collect();
        let statements = inserts(T, &rows);
        assert_eq!(statements.len(), 3, "two rows of 40 kB to a statement");
        assert!(statements.iter().all(|s| s.len() <= MAX_STATEMENT_BYTES));
        assert!(statements[0].starts_with("INSERT OR IGNORE INTO t (release, id, name) VALUES\n"));
        assert!(statements[2].ends_with(");\n"));
        assert_eq!(inserts(T, &[]), Vec::<String>::new());
    }
}
