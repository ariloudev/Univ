//! The statements of the migration: rows written with the releases of their parent, the columns
//! that replace a JSON column gathered in temporary tables, and a table rebuilt in schema 8's
//! shape from its own rows and those columns.

use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;
use rusqlite::types::Value as Sql;
use univ_model::release::{RETIRED, Row, SCHEMA_7_JSON, Scope, SqlValue, TABLES, Table};

use crate::publish::create_as;

/// An error of statement `sql`, as text.
pub(super) fn sql_err<E: std::fmt::Display>(sql: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{sql}: {e}")
}

fn bound(value: SqlValue) -> Sql {
    match value {
        SqlValue::Null => Sql::Null,
        SqlValue::Integer(n) => Sql::Integer(n),
        SqlValue::Real(x) => Sql::Real(x),
        SqlValue::Text(t) => Sql::Text(t),
    }
}

/// The rows a migration writes into `db`, each table's statement made once.
pub(super) struct Inserts<'d> {
    db: &'d Connection,
    sql: HashMap<&'static str, String>,
}

impl<'d> Inserts<'d> {
    pub(super) fn new(db: &'d Connection) -> Self {
        Self {
            db,
            sql: HashMap::new(),
        }
    }

    /// Writes `rows` into their table, each retired at `retired`.
    pub(super) fn rows<R: Row>(&mut self, rows: &[R], retired: Option<u32>) -> Result<(), String> {
        self.each(rows.iter().map(|r| (r, retired)))
    }

    /// Writes each row into its table, retired at its own `retired` where the table's rows are
    /// shared between releases.
    pub(super) fn each<'r, R: Row + 'r>(
        &mut self,
        rows: impl IntoIterator<Item = (&'r R, Option<u32>)>,
    ) -> Result<(), String> {
        let table = R::TABLE;
        let shared = table.scope == Scope::PerRelease;
        let sql = self.sql.entry(table.name).or_insert_with(|| {
            let mut columns = table.columns.to_vec();
            if shared {
                columns.push(RETIRED);
            }
            format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table.name,
                columns.join(", "),
                vec!["?"; columns.len()].join(", ")
            )
        });
        let mut q = self.db.prepare_cached(sql).map_err(sql_err(sql))?;
        for (row, retired) in rows {
            let retired = shared.then(|| retired.map_or(Sql::Null, |r| Sql::Integer(r.into())));
            let values = row.values().into_iter().map(bound).chain(retired);
            q.execute(rusqlite::params_from_iter(values))
                .map_err(sql_err(sql))?;
        }
        Ok(())
    }
}

/// The columns `db`'s table `name` has, none when it has no such table.
pub(super) fn columns_of(db: &Connection, name: &str) -> Result<BTreeSet<String>, String> {
    let sql = format!("SELECT name FROM pragma_table_info('{name}')");
    let mut q = db.prepare(&sql).map_err(sql_err(&sql))?;
    q.query_map([], |r| r.get(0))
        .map_err(sql_err(&sql))?
        .collect::<Result<_, _>>()
        .map_err(sql_err(&sql))
}

/// A temporary table that gathers the columns replacing a JSON column, keyed as its table is.
#[derive(Clone, Copy)]
struct TempTable {
    name: &'static str,
    of: &'static str,
    columns: &'static [&'static str],
    key: &'static [&'static str],
}

/// The columns that replace `entries.extra`'s `via` and `spectral_type`.
const ENTRIES: TempTable = TempTable {
    name: "schema8_entries",
    of: "entries",
    columns: &[
        "via_source",
        "via_identifier",
        "spectral_type",
        "spectral_type_kind",
    ],
    key: &["object_id", "entry", "release"],
};

/// The column that replaces `measurements.extra`'s `published_frame`.
const MEASUREMENTS: TempTable = TempTable {
    name: "schema8_measurements",
    of: "measurements",
    columns: &["published_frame"],
    key: &["object_id", "entry", "quantity", "n", "release"],
};

/// The temporary tables of a database's migration.
pub(super) struct Temp<'d> {
    db: &'d Connection,
}

impl<'d> Temp<'d> {
    /// The temporary tables of `db`, made empty.
    pub(super) fn new(db: &'d Connection) -> Result<Self, String> {
        for t in [ENTRIES, MEASUREMENTS] {
            let sql = format!(
                "DROP TABLE IF EXISTS temp.{0}; CREATE TEMP TABLE {0} ({1}, {2}, \
                 PRIMARY KEY ({1})) WITHOUT ROWID;",
                t.name,
                t.key.join(", "),
                t.columns.join(", ")
            );
            db.execute_batch(&sql).map_err(sql_err(&sql))?;
        }
        Ok(Self { db })
    }

    /// The columns of entry `key` that replace its `extra`'s `via` and `spectral_type`.
    pub(super) fn entry(
        &self,
        (object_id, entry, release): (u64, u32, u32),
        via: (Option<u32>, Option<&str>),
        spectral_type: (Option<&str>, Option<u32>),
    ) -> Result<(), String> {
        let sql = "INSERT INTO temp.schema8_entries VALUES (?, ?, ?, ?, ?, ?, ?)";
        self.db
            .prepare_cached(sql)
            .and_then(|mut q| {
                q.execute(rusqlite::params![
                    object_id,
                    entry,
                    release,
                    via.0,
                    via.1,
                    spectral_type.0,
                    spectral_type.1
                ])
            })
            .map(|_| ())
            .map_err(sql_err(sql))
    }

    /// The published frame of measurement `key`, which replaces its `extra`'s.
    pub(super) fn measurement(
        &self,
        (object_id, entry, quantity, n, release): (u64, u32, u32, u32, u32),
        frame: u32,
    ) -> Result<(), String> {
        let sql = "INSERT INTO temp.schema8_measurements VALUES (?, ?, ?, ?, ?, ?)";
        self.db
            .prepare_cached(sql)
            .and_then(|mut q| {
                q.execute(rusqlite::params![
                    object_id, entry, quantity, n, release, frame
                ])
            })
            .map(|_| ())
            .map_err(sql_err(sql))
    }
}

/// Rebuilds `table` of `db` in schema 8's shape: each of its columns from the old table where
/// it has it, from the temporary table of its replacing columns, or NULL; the old table's
/// other columns, its JSON, left behind.
pub(super) fn rebuild(
    db: &Connection,
    table: &Table,
    old: &BTreeSet<String>,
) -> Result<(), String> {
    let name = table.name;
    let next = format!("{name}__v8");
    let mut columns: Vec<&str> = table.columns.to_vec();
    if table.scope == Scope::PerRelease {
        columns.push(RETIRED);
    }
    let temp = [ENTRIES, MEASUREMENTS].into_iter().find(|t| t.of == name);
    let from = |c: &str| -> String {
        if old.contains(c) {
            format!("o.{c}")
        } else if temp.is_some_and(|t| t.columns.contains(&c)) {
            format!("t.{c}")
        } else {
            "NULL".to_owned()
        }
    };
    let selected: Vec<String> = columns.iter().map(|c| from(c)).collect();
    let join = temp.map_or_else(String::new, |t| {
        format!(" LEFT JOIN temp.{} t USING ({})", t.name, t.key.join(", "))
    });
    let sql = format!(
        "DROP TABLE IF EXISTS {next};\n{};\nINSERT INTO {next} ({}) SELECT {} FROM {name} o{join};\n\
         DROP TABLE {name};\nALTER TABLE {next} RENAME TO {name};\n",
        create_as(table, &next)?,
        columns.join(", "),
        selected.join(", ")
    );
    db.execute_batch(&sql).map_err(sql_err(&sql))
}

/// The tables of schema 8 that held a JSON column in schema 7, which a migration rebuilds.
pub(super) fn rebuilt() -> impl Iterator<Item = &'static Table> {
    TABLES
        .iter()
        .filter(|t| SCHEMA_7_JSON.iter().any(|(name, _)| *name == t.name))
}
