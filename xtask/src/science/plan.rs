//! The scan guard (keep-json-out-of-the-tables-that-grow-with-a-releases-objects): a query of a
//! check whose plan reads a table per object or per catalogue entry whole fails, naming it.

use rusqlite::Connection;
use univ_model::release::TABLES;

use crate::json::tables_named;

/// Each step of `sql`'s plan, with `:release` bound to `release`, that reads a table per object
/// or per catalogue entry whole: a `SCAN`, through an index or not, or a `SEARCH` by the release
/// alone, which in the scratch database, where every row is the release's own, reads as much.
///
/// # Errors
///
/// Fails when `sql` cannot be planned.
pub fn whole_reads(db: &Connection, sql: &str, release: u32) -> Result<Vec<String>, String> {
    let mut plan = db
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .map_err(|e| e.to_string())?;
    if let Some(i) = plan
        .parameter_index(":release")
        .map_err(|e| e.to_string())?
    {
        plan.raw_bind_parameter(i, release)
            .map_err(|e| e.to_string())?;
    }
    let mut steps = plan.raw_query();
    let named = tables_named(sql);
    let mut whole = Vec::new();
    while let Some(row) = steps.next().map_err(|e| e.to_string())? {
        let detail: String = row.get(3).map_err(|e| e.to_string())?;
        let reads_all = detail.starts_with("SCAN ")
            || (detail.starts_with("SEARCH ") && detail.ends_with("(release=?)"));
        let Some(name) = detail.split_whitespace().nth(1).filter(|_| reads_all) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        let tables = named.get(&name).cloned().unwrap_or_default();
        let at_scale = TABLES
            .iter()
            .filter(|t| t.name == name)
            .chain(tables)
            .find(|t| !t.scale.allows_json());
        if let Some(table) = at_scale {
            whole.push(format!(
                "{detail} reads {}, a table {}, whole",
                table.name,
                table.scale.words()
            ));
        }
    }
    Ok(whole)
}
