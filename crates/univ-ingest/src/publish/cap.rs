//! The cap on each D1 database (CON-3), checked before anything is uploaded. Releases share
//! rows, so what a database will hold is the rows it holds already, every one of them valid in
//! some release not pruned, and the rows the release adds: its estimate of every row valid in
//! it, less the rows it keeps from its parent, which are among those held already.
//!
//! The rows held already are counted in D1, table by table, and each is estimated at the bytes
//! the release's own rows of that table average; a table the release holds no row of is
//! estimated at [`UNKNOWN_ROW_BYTES`] a row, which errs high.

use std::collections::BTreeSet;

use univ_model::release::ReleaseManifest;
use univ_model::release::manifest::D1Estimate;
use univ_model::release::shards::Database;

use super::Store;
use super::check::tables_in;
use super::refuse::{Refused, over_cap};

/// The bytes a row is estimated at in a table the release holds no row of.
const UNKNOWN_ROW_BYTES: u64 = 1_024;

/// The rows of each table of `estimate` that `database` holds, in one query, none for a table
/// it lacks.
fn held_rows(
    store: &mut dyn Store,
    database: Database,
    estimate: &D1Estimate,
    tables: &BTreeSet<String>,
) -> Result<Vec<u64>, String> {
    let present: Vec<&str> = estimate
        .tables
        .iter()
        .map(|t| t.table.as_str())
        .filter(|t| tables.contains(*t))
        .collect();
    if present.is_empty() {
        return Ok(vec![0; estimate.tables.len()]);
    }
    // D1 limits the terms of a compound `SELECT`, so scalar subqueries, not a `UNION`.
    let counts: Vec<String> = present
        .iter()
        .enumerate()
        .map(|(i, t)| format!("(SELECT COUNT(*) FROM {t}) AS n{i}"))
        .collect();
    let rows = store.query(database, &format!("SELECT {}", counts.join(", ")))?;
    let row = rows
        .first()
        .ok_or_else(|| format!("{database} gave no count of its rows"))?;
    Ok(estimate
        .tables
        .iter()
        .map(|t| {
            present
                .iter()
                .position(|p| *p == t.table)
                .and_then(|i| row.get(&format!("n{i}")))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        })
        .collect())
}

/// The bytes of the rows `database` holds already that the release does not keep, estimated
/// table by table from `estimate`, the release's own there.
fn others_bytes(
    store: &mut dyn Store,
    database: Database,
    estimate: &D1Estimate,
) -> Result<u64, String> {
    let tables = tables_in(store, database)?;
    let held = held_rows(store, database, estimate, &tables)?;
    let mut bytes = 0;
    for (t, held) in estimate.tables.iter().zip(held) {
        let kept = t.added.map_or(0, |added| t.rows.saturating_sub(added));
        let per_row = t
            .estimated_bytes
            .checked_div(t.rows)
            .unwrap_or(UNKNOWN_ROW_BYTES);
        bytes += held.saturating_sub(kept) * per_row;
    }
    Ok(bytes)
}

/// The releases listed and not pruned other than `number`.
fn other_releases(store: &mut dyn Store, number: u32) -> Result<u64, String> {
    if !tables_in(store, Database::Catalogue)?.contains("releases") {
        return Ok(0);
    }
    let rows = store.query(
        Database::Catalogue,
        &format!(
            "SELECT COUNT(*) AS n FROM releases WHERE pruned_at IS NULL AND number <> {number}"
        ),
    )?;
    Ok(rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

/// Refuses release `manifest` when, in any database it loads, the rows held already that it
/// does not keep and its own estimate pass the cap.
pub(super) fn within_caps(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
) -> Result<(), Refused> {
    let number = manifest.release;
    let releases = other_releases(store, number).map_err(Refused::Query)?;
    let shards = manifest
        .shards
        .iter()
        .map(|s| (Database::Shard(s.index), &s.d1));
    for (database, estimate) in std::iter::once((Database::Catalogue, &manifest.d1)).chain(shards) {
        let others = others_bytes(store, database, estimate).map_err(Refused::Query)?;
        over_cap(
            number,
            database,
            estimate.estimated_bytes,
            (others, releases),
        )?;
    }
    Ok(())
}
