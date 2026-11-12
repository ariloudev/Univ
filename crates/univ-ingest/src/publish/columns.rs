//! How the tables a database holds differ from those of univ-model's schema: the columns a
//! table holds beyond the schema's, which a migration keeps for a later one to read, and the
//! schema's columns it lacks.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use univ_model::release::shards::Database;
use univ_model::release::{RETIRED, Scope, TABLES};

use super::Store;

/// The columns a database's tables hold beyond those of [`univ_model::release::SCHEMA`], each with its declared
/// type, by table: those of a JSON column that schema 8 keeps as rows.
pub(super) type Beyond = BTreeMap<String, Vec<(String, String)>>;

/// How each table of [`Scope::PerRelease`] that `database` holds differs from [`univ_model::release::SCHEMA`]: the
/// columns it holds beyond the schema's, with their types, and the schema's columns it lacks.
pub(super) fn differences(
    store: &mut dyn Store,
    database: Database,
    held: &BTreeSet<String>,
) -> Result<(Beyond, BTreeMap<String, Vec<String>>), String> {
    let mut beyond = Beyond::new();
    let mut lacks = BTreeMap::new();
    for table in TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease && held.contains(t.name))
    {
        let sql = format!("SELECT name, type FROM pragma_table_info('{}')", table.name);
        let has: Vec<(String, String)> = store
            .query(database, &sql)?
            .iter()
            .map(|r| {
                let text = |k: &str| {
                    r.get(k)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                (text("name"), text("type"))
            })
            .collect();
        let more: Vec<(String, String)> = has
            .iter()
            .filter(|(c, _)| c != RETIRED && !table.columns.contains(&c.as_str()))
            .cloned()
            .collect();
        let less: Vec<String> = table
            .columns
            .iter()
            .filter(|c| !has.iter().any(|(h, _)| h == *c))
            .map(|c| (*c).to_owned())
            .collect();
        if !more.is_empty() {
            beyond.insert(table.name.to_owned(), more);
        }
        if !less.is_empty() {
            lacks.insert(table.name.to_owned(), less);
        }
    }
    Ok((beyond, lacks))
}
