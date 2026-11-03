//! A table's fingerprint: what a build computes of a release's rows of one table, and what one
//! SQL query computes of the rows D1 holds once they are loaded, so that a row dropped or altered
//! on the way to D1 is found.
//!
//! The fingerprint is the row count and, for each column, a total SQLite core computes:
//!
//! - a text column's bytes, `total(length(CAST(c AS BLOB)))`, every byte counted, NULs too;
//! - an integer or real column's sum, `total(c)`.
//!
//! NULLs count for nothing. Integer and text totals are exact (below 2^53) and compare exactly;
//! a real column's total, summed in another order on each side, compares within
//! [`REAL_TOLERANCE`] of the sum of its values' magnitudes, which bounds the rounding.

use serde::{Deserialize, Serialize};

use super::sql::{ColumnKind, SqlValue, Table};

/// How far a real column's totals may differ, as a share of the sum of its magnitudes.
pub const REAL_TOLERANCE: f64 = 1e-10;
/// The name the fingerprint query gives the row count.
pub const ROWS: &str = "n_rows";

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

impl Table {
    /// The fingerprint of `rows`, each row's values in the order of [`Table::columns`]: one
    /// total per column.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "totals below 2^53 are exact, and those of a release's rows are"
    )]
    pub fn fingerprint(&self, rows: &[Vec<SqlValue>]) -> Vec<ColumnTotal> {
        self.columns
            .iter()
            .zip(self.kinds)
            .enumerate()
            .map(|(i, (column, kind))| {
                let values = rows.iter().filter_map(|r| r.get(i));
                let (total, magnitude) = match kind {
                    ColumnKind::Text => {
                        let bytes: u64 = values
                            .map(|v| match v {
                                SqlValue::Text(t) => t.len() as u64,
                                _ => 0,
                            })
                            .sum();
                        (bytes as f64, None)
                    }
                    ColumnKind::Integer => {
                        let sum: i128 = values
                            .map(|v| match v {
                                SqlValue::Integer(n) => i128::from(*n),
                                _ => 0,
                            })
                            .sum();
                        (sum as f64, None)
                    }
                    ColumnKind::Real => {
                        let (mut sum, mut magnitude) = (Sum::default(), Sum::default());
                        for v in values {
                            let x = match v {
                                SqlValue::Real(x) => *x,
                                #[expect(
                                    clippy::cast_precision_loss,
                                    reason = "as SQLite reads an integer into a real column"
                                )]
                                SqlValue::Integer(n) => *n as f64,
                                _ => continue,
                            };
                            sum.add(x);
                            magnitude.add(x.abs());
                        }
                        (sum.value(), Some(magnitude.value()))
                    }
                };
                ColumnTotal {
                    column: (*column).to_owned(),
                    total,
                    magnitude,
                }
            })
            .collect()
    }

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
        }
        format!(
            "SELECT {} FROM {} WHERE {filter}",
            terms.join(", "),
            self.name
        )
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
        let f = T.fingerprint(&rows);
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
            "SELECT COUNT(*) AS n_rows, total(release) AS t_release, \
             total(length(CAST(name AS BLOB))) AS t_name, total(x) AS t_x FROM t WHERE release = 2"
        );
    }
}
