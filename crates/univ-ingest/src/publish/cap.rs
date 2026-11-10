//! The cap on each D1 database (CON-3), checked before anything is uploaded. Releases share
//! rows, so what a database will hold is the rows it holds already, every one of them valid in
//! some release not pruned, and the rows the release adds: its estimate of every row valid in
//! it, less the rows it keeps from its parent, which are among those held already.
//!
//! The rows held already are counted in D1, table by table, those in the release's own view
//! left out (its estimate counts them, and a publish resumed has loaded some), and each is
//! estimated at the bytes the release's own rows of that table average; a table the release
//! holds no row of is estimated at [`UNKNOWN_ROW_BYTES`] a row. Like every CON-3 estimate it
//! is an estimate: rows of earlier releases larger than this release's are underestimated.

use univ_model::release::manifest::D1Estimate;
use univ_model::release::shards::Database;
use univ_model::release::{D1_CAP_BYTES, ReleaseManifest, Scope, TABLES};

use super::check::{Tables, counts_of, kept_releases};
use super::refuse::Refused;
use super::{PublishError, Store};

/// The bytes a row is estimated at in a table the release holds no row of.
const UNKNOWN_ROW_BYTES: u64 = 1_024;

/// The bytes of the rows `database`, holding `tables`, holds already that release `number`
/// does not keep, estimated table by table from `estimate`, the release's own there.
fn others_bytes(
    store: &mut dyn Store,
    (database, estimate): (Database, &D1Estimate),
    tables: &Tables,
    number: u32,
) -> Result<u64, String> {
    let held = |t: &str| tables.get(&database).is_some_and(|set| set.contains(t));
    let present: Vec<_> = estimate.tables.iter().filter(|t| held(&t.table)).collect();
    // The rows not in this release's view, each by its table's scope: of a shared table, those
    // other releases added (less, below, those it keeps); of the ledger, those of later
    // releases; of the list of releases, the others'. A resumed publish has loaded some of its
    // own, which its estimate counts already.
    let filters: Vec<(&str, String)> = present
        .iter()
        .map(|t| {
            let scope = TABLES.iter().find(|s| s.name == t.table).map(|s| s.scope);
            let filter = match scope {
                Some(Scope::PerRelease) => format!("release <> {number}"),
                Some(Scope::Ledger) => format!("release > {number}"),
                Some(Scope::Releases) => format!("number <> {number}"),
                None => "1".to_owned(),
            };
            (t.table.as_str(), filter)
        })
        .collect();
    let counts = counts_of(store, database, &filters)?;
    Ok(present
        .iter()
        .zip(counts)
        .map(|(t, held)| {
            let kept = t.added.map_or(0, |added| t.rows.saturating_sub(added));
            let per_row = t
                .estimated_bytes
                .checked_div(t.rows)
                .unwrap_or(UNKNOWN_ROW_BYTES);
            held.saturating_sub(kept).saturating_mul(per_row)
        })
        .fold(0, u64::saturating_add))
}

/// Refuses release `manifest` when, in any database it loads, the rows held already that it
/// does not keep and its own estimate pass the cap. `tables` are the tables each database holds.
pub(super) fn within_caps(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
    tables: &Tables,
) -> Result<(), Refused> {
    let number = manifest.release;
    let shards = manifest
        .shards
        .iter()
        .map(|s| (Database::Shard(s.index), &s.d1));
    for each in std::iter::once((Database::Catalogue, &manifest.d1)).chain(shards) {
        let (database, estimate) = each;
        let others = others_bytes(store, each, tables, number).map_err(Refused::Query)?;
        let this = estimate.estimated_bytes;
        if this.saturating_add(others) > D1_CAP_BYTES {
            let listed = tables
                .get(&Database::Catalogue)
                .is_some_and(|t| t.contains("releases"));
            let releases = if listed {
                kept_releases(store, number).map_err(Refused::Query)?.len() as u64
            } else {
                0
            };
            return Err(Refused::Error(PublishError::OverCap {
                number,
                database,
                this,
                others,
                releases,
            }));
        }
    }
    Ok(())
}
