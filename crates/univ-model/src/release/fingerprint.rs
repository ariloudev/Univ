//! A table's fingerprint: what a build computes of a release's rows of one table, and what one
//! SQL query computes of the rows D1 holds once they are loaded, so that a row dropped or altered
//! on the way to D1 is found.
//!
//! The fingerprint is the row count and, for each column, a total SQLite core computes:
//!
//! - a text column's bytes, `total(length(CAST(c AS BLOB)))`, every byte counted, NULs too;
//! - an integer or real column's sum, `total(c)`;
//! - the number of its values that are not NULL, `COUNT(c)`, since a total cannot tell a NULL
//!   from a 0 or an empty text.
//!
//! Integer and text totals are exact (below 2^53) and compare exactly; a real column's total,
//! summed in another order on each side, compares within [`REAL_TOLERANCE`] of the sum of its
//! values' magnitudes, which bounds the rounding.
//!
//! The small lookup tables ([`CHECKED_BY_CONTENT`]: terms, chains, sources and names), whose
//! rows other rows point into by number, are checked by their content too: every row, in the
//! order of its primary key, in the canonical form [`canonical_row`] writes, whose digest the
//! manifest records. A total cannot see a term renamed to another of the same length.

use serde::{Deserialize, Serialize};

use super::sql::{ColumnKind, SqlValue, Table};

/// How far a real column's totals may differ, as a share of the sum of its magnitudes.
pub const REAL_TOLERANCE: f64 = 1e-10;
/// The name the fingerprint query gives the row count.
pub const ROWS: &str = "n_rows";
/// The tables checked by their content as well as their totals.
pub const CHECKED_BY_CONTENT: [&str; 4] = ["terms", "chains", "sources", "names"];

/// One column's total over a release's rows of a table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnTotal {
    pub column: String,
    /// The bytes of a text column, the sum of a numeric one.
    pub total: f64,
    /// The sum of a real column's magnitudes, which bounds the rounding of its total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,
    /// How many of its values are not NULL; none in a manifest written before it was counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_null: Option<u64>,
}

impl ColumnTotal {
    /// Whether `got`, the total D1 gives, differs from this one beyond its rounding.
    #[must_use]
    pub fn differs(&self, got: f64) -> bool {
        let tolerance = self.magnitude.map_or(0.0, |m| m * REAL_TOLERANCE);
        (got - self.total).abs() > tolerance
    }
}

/// The name the fingerprint query gives `column`'s total.
#[must_use]
pub fn alias(column: &str) -> String {
    format!("t_{column}")
}

/// The name the fingerprint query gives the count of `column`'s values that are not NULL: a
/// prefix of its own, so that it never meets [`ROWS`], which a column named `rows` would.
#[must_use]
pub fn non_null_alias(column: &str) -> String {
    format!("nn_{column}")
}

/// Appends `row` to `out` in the canonical form a content digest takes: each value tagged by
/// its type, a text with its length in bytes, so no two rows write the same bytes.
pub fn canonical_row(row: &[SqlValue], out: &mut Vec<u8>) {
    for value in row {
        match value {
            SqlValue::Null => out.extend_from_slice(b"n;"),
            SqlValue::Integer(n) => out.extend_from_slice(format!("i{n};").as_bytes()),
            SqlValue::Real(x) => {
                out.extend_from_slice(format!("r{:016x};", x.to_bits()).as_bytes());
            }
            SqlValue::Text(t) => {
                out.extend_from_slice(format!("t{}:", t.len()).as_bytes());
                out.extend_from_slice(t.as_bytes());
                out.push(b';');
            }
        }
    }
    out.push(b'\n');
}

/// A compensated (Neumaier) sum, as SQLite's `total()` sums.
#[derive(Default)]
struct Sum {
    sum: f64,
    compensation: f64,
}

impl Sum {
    fn add(&mut self, x: f64) {
        let t = self.sum + x;
        if self.sum.abs() >= x.abs() {
            self.compensation += (self.sum - t) + x;
        } else {
            self.compensation += (x - t) + self.sum;
        }
        self.sum = t;
    }

    fn value(&self) -> f64 {
        self.sum + self.compensation
    }
}

/// One column's running total.
enum Running {
    Bytes(u64),
    Sum(i128),
    Real(Sum, Sum),
}

/// A table's fingerprint taken one row at a time, so no row need be kept.
pub struct Fingerprinter {
    table: Table,
    totals: Vec<Running>,
    non_null: Vec<u64>,
}

impl Fingerprinter {
    /// An empty fingerprint of `table`.
    #[must_use]
    pub fn new(table: Table) -> Self {
        let totals = table
            .kinds
            .iter()
            .map(|kind| match kind {
                ColumnKind::Text => Running::Bytes(0),
                ColumnKind::Integer => Running::Sum(0),
                ColumnKind::Real => Running::Real(Sum::default(), Sum::default()),
            })
            .collect();
        let non_null = vec![0; table.columns.len()];
        Self {
            table,
            totals,
            non_null,
        }
    }

    /// Adds a row's values, in the order of [`Table::columns`].
    pub fn add(&mut self, row: &[SqlValue]) {
        self.add_values(row.iter());
    }

    /// Adds a row's values with the one at `at` replaced by `value`: a row a release shares
    /// with an earlier one, counted with the release that added it, without a copy of it.
    pub fn add_replacing(&mut self, row: &[SqlValue], at: usize, value: &SqlValue) {
        self.add_values(
            row.iter()
                .enumerate()
                .map(|(i, v)| if i == at { value } else { v }),
        );
    }

    fn add_values<'v>(&mut self, row: impl Iterator<Item = &'v SqlValue> + Clone) {
        for (count, value) in self.non_null.iter_mut().zip(row.clone()) {
            *count += u64::from(!matches!(value, SqlValue::Null));
        }
        for (total, value) in self.totals.iter_mut().zip(row) {
            match (total, value) {
                (Running::Bytes(b), SqlValue::Text(t)) => *b += t.len() as u64,
                (Running::Sum(s), SqlValue::Integer(n)) => *s += i128::from(*n),
                (Running::Real(sum, magnitude), SqlValue::Real(x)) => {
                    sum.add(*x);
                    magnitude.add(x.abs());
                }
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "as SQLite reads an integer into a real column"
                )]
                (Running::Real(sum, magnitude), SqlValue::Integer(n)) => {
                    let x = *n as f64;
                    sum.add(x);
                    magnitude.add(x.abs());
                }
                _ => {}
            }
        }
    }

    /// One total per column.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "totals below 2^53 are exact, and those of a release's rows are"
    )]
    pub fn finish(&self) -> Vec<ColumnTotal> {
        self.table
            .columns
            .iter()
            .zip(&self.totals)
            .zip(&self.non_null)
            .map(|((column, running), non_null)| {
                let (total, magnitude) = match running {
                    Running::Bytes(b) => (*b as f64, None),
                    Running::Sum(s) => (*s as f64, None),
                    Running::Real(sum, magnitude) => (sum.value(), Some(magnitude.value())),
                };
                ColumnTotal {
                    column: (*column).to_owned(),
                    total,
                    magnitude,
                    non_null: Some(*non_null),
                }
            })
            .collect()
    }
}

impl Table {
    /// The query that gives the fingerprint of the rows `filter` selects, in one row: the count
    /// as [`ROWS`] and each column's total by its [`alias`].
    #[must_use]
    pub fn fingerprint_query(&self, filter: &str) -> String {
        let mut terms = vec![format!("COUNT(*) AS {ROWS}")];
        for (column, kind) in self.columns.iter().zip(self.kinds) {
            let measure = match kind {
                ColumnKind::Text => format!("length(CAST({column} AS BLOB))"),
                ColumnKind::Integer | ColumnKind::Real => (*column).to_owned(),
            };
            terms.push(format!("total({measure}) AS {}", alias(column)));
            terms.push(format!("COUNT({column}) AS {}", non_null_alias(column)));
        }
        format!(
            "SELECT {} FROM {} WHERE {filter}",
            terms.join(", "),
            self.name
        )
    }

    /// Whether the table is checked by its content as well as its totals.
    #[must_use]
    pub fn checked_by_content(&self) -> bool {
        CHECKED_BY_CONTENT.contains(&self.name)
    }

    /// The query that gives every row `filter` selects, its columns in order, in the order of
    /// the primary key: the order [`Table::sort_by_key`] puts rows in.
    #[must_use]
    pub fn content_query(&self, filter: &str) -> String {
        format!(
            "SELECT {} FROM {} WHERE {filter} ORDER BY {}",
            self.columns.join(", "),
            self.name,
            self.key.join(", ")
        )
    }

    /// Sorts `rows` by their primary key as `SQLite` orders it: a number before a text, numbers
    /// by value and texts by their bytes.
    pub fn sort_by_key(&self, rows: &mut [Vec<SqlValue>]) {
        let places: Vec<usize> = self.key.iter().filter_map(|c| self.position(c)).collect();
        rows.sort_by(|a, b| {
            places
                .iter()
                .map(|&i| order(&a[i], &b[i]))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Two values as `SQLite` orders them: NULL first, then numbers, then texts by their bytes.
fn order(a: &SqlValue, b: &SqlValue) -> std::cmp::Ordering {
    let class = |v: &SqlValue| match v {
        SqlValue::Null => 0,
        SqlValue::Integer(_) | SqlValue::Real(_) => 1,
        SqlValue::Text(_) => 2,
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "as SQLite compares an integer with a real"
    )]
    let number = |v: &SqlValue| match v {
        SqlValue::Integer(n) => *n as f64,
        SqlValue::Real(x) => *x,
        _ => 0.0,
    };
    match (a, b) {
        (SqlValue::Integer(x), SqlValue::Integer(y)) => x.cmp(y),
        (SqlValue::Text(x), SqlValue::Text(y)) => x.as_bytes().cmp(y.as_bytes()),
        _ if class(a) == class(b) => number(a).total_cmp(&number(b)),
        _ => class(a).cmp(&class(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::sql::Scope;

    const T: Table = Table {
        name: "t",
        columns: &["release", "name", "x"],
        kinds: &[ColumnKind::Integer, ColumnKind::Text, ColumnKind::Real],
        key: &["release", "name"],
        indexes: &[],
        scope: Scope::PerRelease,
    };

    #[test]
    fn a_fingerprint_totals_each_column_as_its_query_does() {
        let rows = vec![
            vec![
                SqlValue::Integer(2),
                SqlValue::from("é\0b"),
                SqlValue::Real(0.1),
            ],
            vec![SqlValue::Integer(2), SqlValue::Null, SqlValue::Real(-0.3)],
        ];
        let mut fingerprinter = Fingerprinter::new(T);
        for row in &rows {
            fingerprinter.add(row);
        }
        let f = fingerprinter.finish();
        assert_eq!(f[0].total, 4.0);
        assert_eq!(
            (f[1].total, f[1].magnitude),
            (4.0, None),
            "bytes, the NUL too"
        );
        assert!((f[2].total + 0.2).abs() < 1e-15);
        assert!(f[2].differs(-0.2 + 1e-9) && !f[2].differs(-0.2 + 1e-12));
        assert_eq!(
            T.fingerprint_query("release = 2"),
            "SELECT COUNT(*) AS n_rows, total(release) AS t_release, COUNT(release) AS \
             nn_release, total(length(CAST(name AS BLOB))) AS t_name, COUNT(name) AS nn_name, \
             total(x) AS t_x, COUNT(x) AS nn_x FROM t WHERE release = 2"
        );
    }

    #[test]
    fn every_measure_of_every_table_has_a_name_of_its_own() {
        // The payloads table has a column named rows: its count must not take the name of the
        // row count.
        for table in crate::release::TABLES {
            let mut names = vec![ROWS.to_owned()];
            for column in table.columns {
                names.push(alias(column));
                names.push(non_null_alias(column));
            }
            let all = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), all, "{}", table.name);
        }
    }

    #[test]
    fn a_null_is_told_from_a_zero_by_the_count_of_values_not_null() {
        let row = |x: SqlValue| vec![SqlValue::Integer(0), SqlValue::from(""), x];
        let fingerprint = |rows: &[Vec<SqlValue>]| {
            let mut f = Fingerprinter::new(T);
            for r in rows {
                f.add(r);
            }
            f.finish()
        };
        let zero = fingerprint(&[row(SqlValue::Real(0.0))]);
        let null = fingerprint(&[row(SqlValue::Null)]);
        assert_eq!(
            zero[2].total, null[2].total,
            "a total cannot tell them apart"
        );
        assert_eq!((zero[2].non_null, null[2].non_null), (Some(1), Some(0)));
        assert_eq!((zero[0].non_null, zero[1].non_null), (Some(1), Some(1)));
    }

    #[test]
    fn a_text_renamed_to_one_of_the_same_length_changes_the_canonical_rows() {
        let canonical = |rows: &[Vec<SqlValue>]| {
            let mut out = Vec::new();
            for r in rows {
                canonical_row(r, &mut out);
            }
            out
        };
        let a = [vec![
            SqlValue::Integer(2),
            SqlValue::from("cepheids"),
            SqlValue::Null,
        ]];
        let b = [vec![
            SqlValue::Integer(2),
            SqlValue::from("cepheidz"),
            SqlValue::Null,
        ]];
        assert_ne!(canonical(&a), canonical(&b));
        // A NULL, a zero and an empty text each write their own bytes.
        let null = [vec![SqlValue::Null, SqlValue::Null, SqlValue::Null]];
        let zero = [vec![
            SqlValue::Integer(0),
            SqlValue::from(""),
            SqlValue::Real(0.0),
        ]];
        assert_ne!(canonical(&null), canonical(&zero));
        // Rows sort by the key as SQLite orders it: numbers by value, texts by their bytes.
        let mut rows = vec![
            vec![SqlValue::Integer(10), SqlValue::from("a"), SqlValue::Null],
            vec![SqlValue::Integer(2), SqlValue::from("b"), SqlValue::Null],
            vec![SqlValue::Integer(2), SqlValue::from("B"), SqlValue::Null],
        ];
        T.sort_by_key(&mut rows);
        let keys: Vec<_> = rows.iter().map(|r| (r[0].clone(), r[1].clone())).collect();
        assert_eq!(
            keys,
            [
                (SqlValue::Integer(2), SqlValue::from("B")),
                (SqlValue::Integer(2), SqlValue::from("b")),
                (SqlValue::Integer(10), SqlValue::from("a"))
            ]
        );
        assert!(!T.checked_by_content());
        assert_eq!(
            T.content_query("release = 2"),
            "SELECT release, name, x FROM t WHERE release = 2 ORDER BY release, name"
        );
    }
}
