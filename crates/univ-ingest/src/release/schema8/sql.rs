//! The statements of the migration: rows written with the releases of their parent, the columns
//! that replace a JSON column gathered in temporary tables, and a table rebuilt in schema 8's
//! shape from its own rows and those columns.

use std::collections::BTreeSet;

use rusqlite::Connection;
use rusqlite::types::Value as Sql;
use univ_model::release::{RETIRED, Row, Scope, SqlValue, TABLES, Table};

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

/// Writes `rows` into their table of `db`, each retired at `retired` where the table's rows are
/// shared between releases.
pub(super) fn insert<R: Row>(
    db: &Connection,
    rows: &[R],
    retired: Option<u32>,
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let table = R::TABLE;
    let shared = table.scope == Scope::PerRelease;
    let mut columns = table.columns.to_vec();
    if shared {
        columns.push(RETIRED);
    }
    let marks = vec!["?"; columns.len()].join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({marks})",
        table.name,
        columns.join(", ")
    );
    let mut q = db.prepare_cached(&sql).map_err(sql_err(&sql))?;
    for row in rows {
        let mut values: Vec<Sql> = row.values().into_iter().map(bound).collect();
        if shared {
            values.push(retired.map_or(Sql::Null, |r| Sql::Integer(i64::from(r))));
        }
        q.execute(rusqlite::params_from_iter(values))
            .map_err(sql_err(&sql))?;
    }
    Ok(())
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

/// The temporary tables that gather the columns replacing `entries.extra`'s `via` and
/// `spectral_type` and `measurements.extra`'s `published_frame`, each keyed as its table is.
pub(super) struct Temp<'d> {
    db: &'d Connection,
}

/// The temporary tables' names, columns and keys.
const ENTRIES: (&str, &[&str], &[&str]) = (
    "schema8_entries",
    &[
        "via_source",
        "via_identifier",
        "spectral_type",
        "spectral_type_kind",
    ],
    &["object_id", "entry", "release"],
);
const MEASUREMENTS: (&str, &[&str], &[&str]) = (
    "schema8_measurements",
    &["published_frame"],
    &["object_id", "entry", "quantity", "n", "release"],
);

impl<'d> Temp<'d> {
    /// The temporary tables of `db`, made empty.
    pub(super) fn new(db: &'d Connection) -> Result<Self, String> {
        for (name, columns, key) in [ENTRIES, MEASUREMENTS] {
            let sql = format!(
                "DROP TABLE IF EXISTS temp.{name}; CREATE TEMP TABLE {name} ({}, {}, \
                 PRIMARY KEY ({})) WITHOUT ROWID;",
                key.join(", "),
                columns.join(", "),
                key.join(", ")
            );
            db.execute_batch(&sql).map_err(sql_err(&sql))?;
        }
        Ok(Self { db })
    }

    /// The columns of entry `key` that replace its `extra`'s `via` and `spectral_type`.
    pub(super) fn entry(
        &self,
        (object_id, entry, release): (u64, u32, u32),
        via_source: Option<u32>,
        via_identifier: Option<&str>,
        spectral_type: Option<&str>,
        spectral_type_kind: Option<u32>,
    ) -> Result<(), String> {
        let sql = "INSERT INTO temp.schema8_entries VALUES (?, ?, ?, ?, ?, ?, ?)";
        self.db
            .prepare_cached(sql)
            .and_then(|mut q| {
                q.execute(rusqlite::params![
                    object_id,
                    entry,
                    release,
                    via_source,
                    via_identifier,
                    spectral_type,
                    spectral_type_kind
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

/// The temporary table that holds columns of `table`, if any.
fn temp_of(
    table: &str,
) -> Option<(
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
)> {
    match table {
        "entries" => Some(ENTRIES),
        "measurements" => Some(MEASUREMENTS),
        _ => None,
    }
}

/// Rebuilds `table` of `db` in schema 8's shape: each of its columns from the old table where
/// it has it, from the temporary table of its replacing columns, or NULL; the old table's
/// other columns, its JSON, left behind.
pub(super) fn rebuild(db: &Connection, table: &Table) -> Result<(), String> {
    let name = table.name;
    let old = columns_of(db, name)?;
    let next = format!("{name}__v8");
    let mut columns: Vec<&str> = table.columns.to_vec();
    if table.scope == Scope::PerRelease {
        columns.push(RETIRED);
    }
    let temp = temp_of(name);
    let from = |c: &str| -> String {
        if old.contains(c) {
            format!("o.{c}")
        } else if temp.is_some_and(|(_, held, _)| held.contains(&c)) {
            format!("t.{c}")
        } else {
            "NULL".to_owned()
        }
    };
    let selected: Vec<String> = columns.iter().map(|c| from(c)).collect();
    let join = temp.map_or_else(String::new, |(t, _, key)| {
        format!(" LEFT JOIN temp.{t} t USING ({})", key.join(", "))
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

/// The tables of schema 8 a migration rebuilds, each with a column schema 7 held as JSON.
pub(super) fn rebuilt() -> impl Iterator<Item = &'static Table> {
    TABLES
        .iter()
        .filter(|t| ["objects", "entries", "measurements", "ids_replaced"].contains(&t.name))
}
