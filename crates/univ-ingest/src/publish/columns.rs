//! The columns, with their declared types, of the tables a database holds, which a migration
//! compares with univ-model's schema: a column beyond the schema's is kept for a later
//! migration to read, and one of the schema's a table lacks is NULL.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use univ_model::release::shards::Database;
use univ_model::release::{Scope, TABLES};

use super::Store;

/// Each table's columns as a database holds them, each with its declared type, by table.
pub(super) type Columns = BTreeMap<String, Vec<(String, String)>>;

/// The columns of each table of [`Scope::PerRelease`] that `database` holds of `held`.
pub(super) fn columns(
    store: &mut dyn Store,
    database: Database,
    held: &BTreeSet<String>,
) -> Result<Columns, String> {
    let mut columns = Columns::new();
    for table in TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease && held.contains(t.name))
    {
        let sql = format!("SELECT name, type FROM pragma_table_info('{}')", table.name);
        let has = store
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
        columns.insert(table.name.to_owned(), has);
    }
    Ok(columns)
}
