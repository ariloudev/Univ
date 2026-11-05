//! What refuses a publish before anything is uploaded: the release already published, pruned
//! or older than `latest`'s; more object shards than Univ Server binds; or, in any database,
//! its rows and those of the releases not pruned estimated beyond the CON-3 cap.

use std::collections::BTreeMap;

use univ_model::release::shards::Database;
use univ_model::release::{D1_CAP_BYTES, ReleaseManifest};

use super::check::published_status;
use super::upload::record_published;
use super::{Latest, PublishError, QueryRow, Store};

/// Refuses a release that needs more object shards than the store's Univ Server binds.
pub(super) fn unbound(store: &mut dyn Store, manifest: &ReleaseManifest) -> Result<(), Refused> {
    let needs = u32::try_from(manifest.shards.len()).unwrap_or(u32::MAX);
    let binds = store.bound_shards().map_err(Refused::Query)?;
    if needs > binds {
        return Err(Refused::Error(PublishError::TooManyShards {
            number: manifest.release,
            needs,
            binds,
        }));
    }
    Ok(())
}

/// The sum of `bytes` and the count of `n` that `row` gives.
fn sum_and_count(row: Option<&QueryRow>) -> (u64, u64) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a sum of byte counts, whole and positive"
    )]
    let bytes = row
        .and_then(|r| r.get("bytes"))
        .and_then(serde_json::Value::as_f64)
        .map_or(0, |b| b as u64);
    let n = row
        .and_then(|r| r.get("n"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    (bytes, n)
}

/// Checks each database the release `manifest` loads against the cap (CON-3): its estimate
/// there with those of every other release the catalogue lists that is not pruned. The
/// catalogue's are the releases' `d1_estimated_bytes` (all of a release's rows, for one built
/// before object shards), a shard's their `release_shards` rows, all shards in one query;
/// `holds` says which of those tables the catalogue holds.
pub(super) fn within_caps(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
    holds: Holds,
) -> Result<(), Refused> {
    let number = manifest.release;
    let others = format!("pruned_at IS NULL AND number <> {number}");
    let mut query = |sql: String| {
        store
            .query(Database::Catalogue, &sql)
            .map_err(Refused::Query)
    };
    let catalogue = if holds.releases {
        let rows = query(format!(
            "SELECT total(d1_estimated_bytes) AS bytes, COUNT(*) AS n FROM releases WHERE {others}"
        ))?;
        sum_and_count(rows.first())
    } else {
        (0, 0)
    };
    let this = manifest.d1.estimated_bytes;
    over_cap(number, Database::Catalogue, this, catalogue)?;
    let mut earlier = BTreeMap::new();
    if holds.shard_index && !manifest.shards.is_empty() {
        let rows = query(format!(
            "SELECT shard, total(estimated_bytes) AS bytes, COUNT(*) AS n FROM release_shards \
             WHERE release IN (SELECT number FROM releases WHERE {others}) GROUP BY shard"
        ))?;
        for row in &rows {
            if let Some(shard) = row.get("shard").and_then(serde_json::Value::as_u64) {
                earlier.insert(shard, sum_and_count(Some(row)));
            }
        }
    }
    for shard in &manifest.shards {
        let others = earlier.get(&u64::from(shard.index)).copied();
        let database = Database::Shard(shard.index);
        over_cap(
            number,
            database,
            shard.d1.estimated_bytes,
            others.unwrap_or((0, 0)),
        )?;
    }
    Ok(())
}

/// Which of the tables the cap is checked by the catalogue holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Holds {
    /// The list of releases.
    pub releases: bool,
    /// The index of the object shards.
    pub shard_index: bool,
}

/// Refuses release `number` when its estimate `this` in `database` and `others`, the summed
/// estimates there of the other releases not pruned and how many they are, pass the cap.
fn over_cap(
    number: u32,
    database: Database,
    this: u64,
    (others, releases): (u64, u64),
) -> Result<(), Refused> {
    if this + others > D1_CAP_BYTES {
        return Err(Refused::Error(PublishError::OverCap {
            number,
            database,
            this,
            others,
            releases,
        }));
    }
    Ok(())
}

/// Why publishing was refused before anything was uploaded.
pub(super) enum Refused {
    Error(PublishError),
    Query(String),
}

/// Where `latest` points, and which tables of releases the catalogue holds, once release `number` is found
/// neither published nor pruned, nor older than the release `latest` points to. A release whose publication time is missing is
/// recorded as published, and refused as such, when `latest` points to it (at the time it moved
/// there) or when `done_at` gives the time the journal says it was whole, whatever `latest`
/// points to.
pub(super) fn refuse_published(
    store: &mut dyn Store,
    number: u32,
    done_at: Option<String>,
) -> Result<(Latest, Holds), Refused> {
    let status = published_status(store, number).map_err(Refused::Query)?;
    let latest = status.latest();
    let holds = Holds {
        releases: status.schema,
        shard_index: status.shard_index,
    };
    let refused = |e| Err(Refused::Error(e));
    if let Some(published_at) = status.published_at {
        return refused(PublishError::AlreadyPublished {
            number,
            published_at,
            latest,
        });
    }
    if let Some(pruned_at) = status.pruned_at {
        return refused(PublishError::Pruned { number, pruned_at });
    }
    let recovered = match (&status.latest, done_at) {
        // Moved by a run stopped before it recorded the publication time.
        (Some((n, moved_at)), _) if *n == number => Some(moved_at.clone()),
        // Whole by its journal, and a newer release published since.
        (Some((n, _)), Some(at)) if *n > number && status.listed => Some(at),
        _ => None,
    };
    if let Some(published_at) = recovered {
        record_published(store, number, &published_at).map_err(Refused::Query)?;
        return refused(PublishError::AlreadyPublished {
            number,
            published_at,
            latest,
        });
    }
    match status.latest {
        Some((n, _)) if n > number => refused(PublishError::NotNewer { number, latest: n }),
        _ => Ok((latest, holds)),
    }
}
