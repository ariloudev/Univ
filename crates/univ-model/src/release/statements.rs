//! SQL statements made one row at a time, each closed once the next row would take it over
//! [`MAX_STATEMENT_BYTES`]: a table's `INSERT OR IGNORE` statements, or any statement that lists
//! tuples after a head, such as the `UPDATE … IN (VALUES …)` that retires shared rows.

use super::sql::{MAX_STATEMENT_BYTES, SqlValue, Table, write_tuple};

/// Statements made one tuple at a time: a statement is given once the next tuple would take it
/// over [`MAX_STATEMENT_BYTES`].
pub struct Statements {
    head: String,
    /// What closes a statement after its last tuple.
    tail: &'static str,
    current: String,
    /// The row being added, as its tuple: one buffer for every row.
    tuple: String,
}

impl Statements {
    /// No statement yet, into `table`.
    #[must_use]
    pub fn new(table: Table) -> Self {
        Self::with(table.insert_head(), ";\n")
    }

    /// No statement yet: each to begin with `head` and end with `tail`, its tuples between.
    #[must_use]
    pub fn with(head: String, tail: &'static str) -> Self {
        Self {
            head,
            tail,
            current: String::new(),
            tuple: String::new(),
        }
    }

    /// Adds a row, giving the statement it closed, if any.
    pub fn push(&mut self, row: &[SqlValue]) -> Option<String> {
        let mut tuple = std::mem::take(&mut self.tuple);
        tuple.clear();
        write_tuple(row, &mut tuple);
        let closed = self.push_tuple(&tuple);
        self.tuple = tuple;
        closed
    }

    /// Adds a tuple written already, giving the statement it closed, if any.
    pub fn push_tuple(&mut self, tuple: &str) -> Option<String> {
        let mut closed = None;
        if !self.current.is_empty()
            && self.current.len() + tuple.len() + 2 + self.tail.len() > MAX_STATEMENT_BYTES
        {
            self.current.push_str(self.tail);
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
        self.current.push_str(self.tail);
        Some(std::mem::take(&mut self.current))
    }
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
    use crate::release::sql::{ColumnKind, Scope};

    const T: Table = Table {
        name: "t",
        columns: &["release", "id", "name"],
        kinds: &[ColumnKind::Integer, ColumnKind::Integer, ColumnKind::Text],
        key: &["release", "id"],
        indexes: &[],
        scope: Scope::PerRelease,
    };

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

    #[test]
    fn a_statement_of_tuples_after_a_head_closes_with_its_tail() {
        let mut s = Statements::with(
            "UPDATE t SET retired = 3 WHERE (id) IN (VALUES ".into(),
            ");\n",
        );
        assert_eq!(s.push_tuple("(1)"), None);
        assert_eq!(s.push_tuple("(2)"), None);
        assert_eq!(
            s.finish().as_deref(),
            Some("UPDATE t SET retired = 3 WHERE (id) IN (VALUES (1),\n(2));\n")
        );
    }
}
