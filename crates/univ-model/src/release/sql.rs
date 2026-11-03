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
//! overhead. The sum is divided by [`PAGE_FILL`], the share of a B-tree page that holds cells,
//! so the estimate errs high rather than low.

use std::fmt::Write as _;

/// The share of a B-tree page an estimate takes as holding cells: pages split by inserts are
/// often only two-thirds full.
pub const PAGE_FILL: f64 = 0.65;
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
    /// The value as an SQL literal: a text quoted with its quotes doubled (one holding a NUL,
    /// which a literal cannot, as a cast of its hexadecimal bytes), a real as the shortest
    /// decimal that reads back the same, a real that is not finite as NULL.
    #[must_use]
    pub fn literal(&self) -> String {
        match self {
            Self::Integer(n) => n.to_string(),
            Self::Real(x) if x.is_finite() => format!("{x:?}"),
            Self::Null | Self::Real(_) => "NULL".to_owned(),
            Self::Text(t) if t.contains('\0') => {
                let hex = t.bytes().fold(String::new(), |mut s, b| {
                    let _ = write!(s, "{b:02X}");
                    s
                });
                format!("CAST(X'{hex}' AS TEXT)")
            }
            Self::Text(t) => format!("'{}'", t.replace('\'', "''")),
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

/// The bytes of a record of `values`, with its cell overhead.
fn record_bytes<'a>(values: impl IntoIterator<Item = &'a SqlValue>) -> u64 {
    let (header, body) = values
        .into_iter()
        .map(SqlValue::stored)
        .fold((0, 0), |(h, b), (sh, sb)| (h + sh, b + sb));
    let header = header + varint_bytes(header + 1);
    header + body + 4
}

/// A table of the schema: its columns in the order rows give them, its primary key and its
/// secondary indexes, each by column names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Table {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub key: &'static [&'static str],
    pub indexes: &'static [&'static [&'static str]],
    /// Whether it holds one release's rows, with a `release` column leading its key; the
    /// others (the list of releases, `latest` and the id ledger) are kept across releases.
    pub per_release: bool,
}

impl Table {
    fn position(&self, column: &str) -> Option<usize> {
        self.columns.iter().position(|c| *c == column)
    }

    /// The estimated D1 bytes of a row of `values`, in the order of [`Table::columns`]: the
    /// row's record and one per secondary index, over [`PAGE_FILL`].
    #[must_use]
    pub fn estimated_bytes(&self, values: &[SqlValue]) -> u64 {
        let mut bytes = record_bytes(values);
        for index in self.indexes {
            let columns = index.iter().chain(self.key.iter());
            let picked = columns.filter_map(|c| self.position(c).and_then(|i| values.get(i)));
            bytes += record_bytes(picked);
        }
        // Rounded up: the estimate errs high.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a row's bytes are far below 2^52 and the ratio is positive"
        )]
        let total = (bytes as f64 / PAGE_FILL).ceil() as u64;
        total
    }

    /// `INSERT OR IGNORE` statements for `rows`, each row's values in the order of
    /// [`Table::columns`], several rows to a statement and no statement over
    /// [`MAX_STATEMENT_BYTES`] unless one row alone is longer.
    #[must_use]
    pub fn inserts(&self, rows: &[Vec<SqlValue>]) -> Vec<String> {
        let head = format!(
            "INSERT OR IGNORE INTO {} ({}) VALUES\n",
            self.name,
            self.columns.join(", ")
        );
        let mut statements = Vec::new();
        let mut current = String::new();
        for row in rows {
            let values: Vec<String> = row.iter().map(SqlValue::literal).collect();
            let tuple = format!("({})", values.join(", "));
            if !current.is_empty() && current.len() + tuple.len() + 3 > MAX_STATEMENT_BYTES {
                current.push_str(";\n");
                statements.push(std::mem::take(&mut current));
            }
            if current.is_empty() {
                current.push_str(&head);
            } else {
                current.push_str(",\n");
            }
            current.push_str(&tuple);
        }
        if !current.is_empty() {
            current.push_str(";\n");
            statements.push(current);
        }
        statements
    }
}

/// A row of a table of the schema.
pub trait Row {
    /// The table it is a row of.
    const TABLE: Table;
    /// Its values in the order of the table's columns.
    fn values(&self) -> Vec<SqlValue>;
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

    const T: Table = Table {
        name: "t",
        columns: &["release", "id", "name"],
        key: &["release", "id"],
        indexes: &[&["name"]],
        per_release: true,
    };

    #[test]
    fn a_row_is_estimated_as_sqlite_stores_it_with_its_index() {
        let row = vec![
            SqlValue::Integer(1),
            SqlValue::Integer(300),
            SqlValue::from("abcd"),
        ];
        // Record: header 1 + 3 serial types, body 0 + 2 + 4, cell 4: 14. Index on (name,
        // release, id): header 4, body 4 + 0 + 2, cell 4: 14. 28 / 0.65 = 43.08, rounded up.
        assert_eq!(T.estimated_bytes(&row), 44);
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
        let statements = T.inserts(&rows);
        assert_eq!(statements.len(), 3, "two rows of 40 kB to a statement");
        assert!(statements.iter().all(|s| s.len() <= MAX_STATEMENT_BYTES));
        assert!(statements[0].starts_with("INSERT OR IGNORE INTO t (release, id, name) VALUES\n"));
        assert!(statements[2].ends_with(");\n"));
        assert_eq!(T.inserts(&[]), Vec::<String>::new());
    }
}
