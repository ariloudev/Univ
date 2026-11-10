//! The files a release shares with its parent, which R2 holds already under their SHA-256: no
//! unit uploads them. The parent is the release `latest` points to, which is never pruned, so
//! R2 keeps them while the release is published, and once it is, the release's own map of its
//! paths holds them.

use std::collections::BTreeSet;

use univ_model::release::shards::Database;
use univ_model::release::{ReleaseManifest, valid_in};

use super::Store;
use super::check::{Tables, sha256s, tables_of};

/// The tables each database release `manifest` loads holds, of the shards the store binds
/// (more are refused), and the SHA-256s of the files its parent maps.
pub(super) fn tables_and_held(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
) -> Result<(Tables, BTreeSet<String>), String> {
    let bound = store.bound_shards()?;
    let needs = u32::try_from(manifest.shards.len()).unwrap_or(u32::MAX);
    let shards = 0..needs.min(bound);
    let databases = std::iter::once(Database::Catalogue).chain(shards.map(Database::Shard));
    let tables = tables_of(store, databases)?;
    let held = held_by(store, manifest.parent, &tables)?;
    Ok((tables, held))
}

/// The SHA-256s of the files the parent of a release maps, by its map of paths in a catalogue
/// holding `tables`; none for a release with no parent.
fn held_by(
    store: &mut dyn Store,
    parent: Option<u32>,
    tables: &Tables,
) -> Result<BTreeSet<String>, String> {
    let mapped = tables
        .get(&Database::Catalogue)
        .is_some_and(|t| t.contains("release_files"));
    match parent.filter(|_| mapped) {
        Some(parent) => sha256s(
            store,
            &format!(
                "SELECT DISTINCT sha256 FROM release_files WHERE {}",
                valid_in("", parent)
            ),
        ),
        None => Ok(BTreeSet::new()),
    }
}
