//! The files a release shares with its parent, which R2 holds already under their SHA-256: they
//! are journaled as held, not uploaded again. The parent is the release `latest` points to,
//! which is never pruned, so R2 keeps them while the release is published.

use std::collections::BTreeSet;

use univ_model::release::manifest::ManifestFile;
use univ_model::release::shards::Database;
use univ_model::release::{ReleaseManifest, valid_in};

use super::journal::{Done, Journal};
use super::progress::Progress;
use super::{Store, StoreOptions};

/// The SHA-256s of the files release `parent` holds, by its map of paths.
fn held_by(store: &mut dyn Store, parent: u32) -> Result<BTreeSet<String>, String> {
    let sql = format!(
        "SELECT DISTINCT sha256 FROM release_files WHERE {}",
        valid_in("", parent)
    );
    store
        .query(Database::Catalogue, &sql)?
        .iter()
        .map(|row| {
            row.get("sha256")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "release_files gives a row with no sha256".to_owned())
        })
        .collect()
}

/// Journals each of the R2 `files` not journaled yet whose SHA-256 is in `held`, the files of
/// release `parent`, as held already, at `at`. Gives how many.
fn journal_held<'f>(
    journal: &mut Journal,
    files: impl Iterator<Item = &'f ManifestFile>,
    (held, parent): (&BTreeSet<String>, u32),
    at: &str,
) -> usize {
    let mut journaled = 0;
    for f in files {
        if held.contains(&f.sha256) && !journal.has(&f.path, &f.sha256) {
            journal.done.push(Done {
                path: f.path.clone(),
                target: f.target,
                key: f.key.clone(),
                sha256: f.sha256.clone(),
                checked: format!("held already by release {parent}, not uploaded again"),
                at: at.to_owned(),
            });
            journaled += 1;
        }
    }
    journaled
}

/// Journals the R2 `files` of release `manifest` that its parent holds already, and says how
/// many: they are in R2 under their SHA-256, and are not uploaded again.
pub(super) fn share<'f>(
    store: &mut dyn Store,
    (manifest, files): (&ReleaseManifest, impl Iterator<Item = &'f ManifestFile>),
    journal: &mut Journal,
    options: &StoreOptions<'_>,
    progress: &mut dyn Progress,
) -> Result<(), String> {
    let Some(parent) = manifest.parent else {
        return Ok(());
    };
    let held = held_by(store, parent)?;
    let shared = journal_held(journal, files, (&held, parent), &(options.now)());
    if shared > 0 {
        journal
            .write(options.journals)
            .map_err(|e| format!("the journal: {e}"))?;
        progress.note(format!(
            "release {}: {shared} R2 files release {parent} holds already, not uploaded again",
            manifest.release
        ));
    }
    Ok(())
}
