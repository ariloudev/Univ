//! A release's units, each uploaded and checked as one: each database's schema, the R2 files
//! R2 does not hold yet, and each database's tables' D1 files; and the check of the tables the
//! release writes no file of.

use std::collections::BTreeSet;

use univ_model::release::manifest::{D1Estimate, ManifestFile, Target};
use univ_model::release::shards::Database;
use univ_model::release::{ReleaseManifest, TABLES, Table, keys};

use super::Store;
use super::check::check_rows;
use super::progress::Progress;

/// A unit uploaded and checked as one: a database's schema, an R2 file, or a table's D1 files
/// in one database.
pub(super) enum Unit<'m> {
    Schema(Database, &'m ManifestFile),
    R2(&'m ManifestFile),
    Table(Database, &'static Table, Vec<&'m ManifestFile>),
}

/// The database and table a D1 file `d1/{dir}/{nnnn}-{table}-{part}.sql` loads, no table for a
/// schema's file.
fn file_of(path: &str) -> Result<(Database, Option<&'static Table>), String> {
    let not_named = || format!("{path} is not named as a D1 file");
    let (dir, file) = path
        .strip_prefix(&format!("{}/", keys::D1_DIR))
        .and_then(|rest| rest.split_once('/'))
        .ok_or_else(not_named)?;
    let database = Database::from_dir(dir).ok_or_else(not_named)?;
    let name = file
        .strip_suffix(".sql")
        .and_then(|p| p.split_once('-'))
        .map(|(_, rest)| rest.rsplit_once('-').map_or(rest, |(table, _)| table))
        .ok_or_else(not_named)?;
    if name == "schema" {
        return Ok((database, None));
    }
    let table = TABLES
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("{path} loads no table of the schema"))?;
    Ok((database, Some(table)))
}

/// The release's units in the order they are uploaded: each database's schema, the R2 files
/// but those whose SHA-256 is `held` already, then each database's tables' D1 files, the
/// catalogue's first, each in the order of the schema's tables.
pub(super) fn units<'m>(
    manifest: &'m ReleaseManifest,
    held: &BTreeSet<String>,
) -> Result<Vec<Unit<'m>>, String> {
    let mut schemas = Vec::new();
    let mut r2 = Vec::new();
    let mut tables: Vec<(Database, &'static Table, Vec<&ManifestFile>)> = Vec::new();
    for f in &manifest.files {
        match f.target {
            // A file kept by its content that the parent holds is in R2 already.
            Target::R2
                if held.contains(&f.sha256)
                    && f.key.as_deref() == Some(keys::file(&f.sha256).as_str()) => {}
            Target::R2 => r2.push(Unit::R2(f)),
            Target::D1 => match file_of(&f.path)? {
                (db, None) => schemas.push((db, f)),
                (db, Some(t)) => match tables
                    .iter_mut()
                    .find(|(d, u, _)| *d == db && u.name == t.name)
                {
                    Some((_, _, files)) => files.push(f),
                    None => tables.push((db, t, vec![f])),
                },
            },
        }
    }
    schemas.sort_by_key(|(db, _)| *db);
    tables.sort_by_key(|(db, t, _)| (*db, TABLES.iter().position(|u| u.name == t.name)));
    let mut units: Vec<Unit<'_>> = schemas
        .into_iter()
        .map(|(db, f)| Unit::Schema(db, f))
        .collect();
    units.extend(r2);
    units.extend(tables.into_iter().map(|(db, t, f)| Unit::Table(db, t, f)));
    Ok(units)
}

/// Checks, against the manifest, the rows valid in the release of each table no unit loads
/// into a database: those whose rows the release all shares with the release before, which it
/// writes no file of, and those it holds none of.
pub(super) fn check_kept(
    store: &mut dyn Store,
    manifest: &ReleaseManifest,
    units: &[Unit<'_>],
    progress: &mut dyn Progress,
) -> Result<(), String> {
    let loaded = |db: Database, name: &str| {
        units
            .iter()
            .any(|u| matches!(u, Unit::Table(d, t, _) if *d == db && t.name == name))
    };
    let shards = (0..manifest.shards.len()).filter_map(|i| u32::try_from(i).ok());
    let mut checked = 0;
    for db in std::iter::once(Database::Catalogue).chain(shards.map(Database::Shard)) {
        let Some(estimate) = estimate_of(manifest, db) else {
            continue;
        };
        for want in estimate.tables.iter().filter(|t| !loaded(db, &t.table)) {
            let table = TABLES
                .iter()
                .find(|t| t.name == want.table)
                .ok_or_else(|| {
                    format!("the manifest names {}, no table of the schema", want.table)
                })?;
            check_rows(store, db, table, manifest.release, want)?;
            checked += 1;
        }
    }
    progress.note(format!(
        "release {}: {checked} tables it writes no file of checked against the manifest",
        manifest.release
    ));
    Ok(())
}

/// The manifest's estimate of the rows of `database`.
pub(super) fn estimate_of(manifest: &ReleaseManifest, database: Database) -> Option<&D1Estimate> {
    match database {
        Database::Catalogue => Some(&manifest.d1),
        Database::Shard(i) => manifest.shards.get(i as usize).map(|s| &s.d1),
    }
}

pub(super) fn unit_files<'u>(unit: &'u Unit<'_>) -> Vec<&'u ManifestFile> {
    match unit {
        Unit::Schema(_, f) | Unit::R2(f) => vec![f],
        Unit::Table(_, _, files) => files.clone(),
    }
}
