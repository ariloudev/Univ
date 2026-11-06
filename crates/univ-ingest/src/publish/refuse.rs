//! What refuses a publish before anything is uploaded: the release already published, pruned
//! or older than `latest`'s; more object shards than Univ Server binds; a database of an older
//! schema version; or a parent latest does not point to. The CON-3 cap is checked in `cap`.

use univ_model::release::ReleaseManifest;

use super::check::{Tables, published_status, schema_version};
use super::upload::record_published;
use super::{Latest, PublishError, Store};

/// Refuses a release written as the change from a parent that `latest` does not point to: its
/// rows would load onto another release's, and its retirements end rows that release holds.
/// A release `latest` points to already, its publication part way, is not refused.
pub(super) fn off_latest(manifest: &ReleaseManifest, latest: Latest) -> Result<(), Refused> {
    let Some(parent) = manifest.parent else {
        return Ok(());
    };
    if latest.0 == Some(parent) || latest.0 == Some(manifest.release) {
        return Ok(());
    }
    Err(Refused::Error(PublishError::NotOnLatest {
        number: manifest.release,
        parent,
        latest: latest.0,
    }))
}

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

/// Refuses a release when a database it loads holds releases of an older schema version than
/// its own: the tables of such a database are not shared between releases, and the release's
/// rows would not load into them. A database that holds no schema yet takes the release's.
pub(super) fn older_schema(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
    tables: &Tables,
) -> Result<(), Refused> {
    let needs = manifest.versions.d1_schema;
    for (database, held) in tables {
        if !held.contains("schema_version") {
            continue;
        }
        let holds = schema_version(store, *database).map_err(Refused::Query)?;
        if let Some(holds) = holds.filter(|&v| v > 0 && v < i64::from(needs)) {
            return Err(Refused::Error(PublishError::OlderSchema {
                number: manifest.release,
                database: database.to_string(),
                holds: u32::try_from(holds).unwrap_or(u32::MAX),
                needs,
            }));
        }
    }
    Ok(())
}

/// Why publishing was refused before anything was uploaded.
pub(super) enum Refused {
    Error(PublishError),
    Query(String),
}

/// Where `latest` points, once release `number` is found neither published nor pruned, nor
/// older than the release `latest` points to. A release whose publication time is missing is
/// recorded as published, and refused as such, when `latest` points to it (at the time it moved
/// there) or when `done_at` gives the time the journal says it was whole, whatever `latest`
/// points to.
pub(super) fn refuse_published(
    store: &mut dyn Store,
    number: u32,
    done_at: Option<String>,
) -> Result<Latest, Refused> {
    let status = published_status(store, number).map_err(Refused::Query)?;
    let latest = status.latest();
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
        _ => Ok(latest),
    }
}
