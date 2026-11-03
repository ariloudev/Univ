//! A release assembled in memory, its rows, files and manifest, and written to and checked on
//! disk.

use std::path::{Path, PathBuf};

use univ_model::derive::{MODEL_VERSION, Release};
use univ_model::release::cells::{MAX_STRUCTURE_LEVEL, SPACES, STRUCTURE_SPAN};
use univ_model::release::manifest::{
    CellIndex, CellSpaceInfo, Counts, D1Estimate, FORMAT, Ids, MANIFEST_VERSION, ManifestFile,
    ManifestSource, NotIndexed, RecordedFile, ReleaseManifest, Target, Versions,
};
use univ_model::release::rows::ReleaseRow;
use univ_model::release::{Row, SCHEMA_VERSION, SqlValue, keys};

use super::check::{Failed, MANIFEST, Subject, check_all};
use super::objects::{ObjectRows, json, ledger_rows, object_rows};
use super::{
    CellRows, D1Rows, GridFiles, Inputs, cell_rows, domain_rows, grid_files, source_rows,
    structure_rows,
};
use crate::crossmatch::RULES;
use crate::tiles::{MANIFEST_VERSION as TILE_MANIFEST_VERSION, name_of, sha256};
use crate::yaml::{FileError, parse_checked};

/// The directory, within the build output, of the releases.
pub const RELEASES: &str = "releases";

/// The directory of release `number` under the build output `out`.
#[must_use]
pub fn release_dir(out: &Path, number: u32) -> PathBuf {
    out.join(RELEASES).join(number.to_string())
}

/// A file of the release: its path in the release directory, its bytes and where it goes.
pub struct ReleaseFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub target: Target,
}

/// A release assembled in memory, with the failures of its checks.
pub struct Assembled {
    pub files: Vec<ReleaseFile>,
    pub manifest: ReleaseManifest,
    pub failed: Vec<Failed>,
}

/// The rules files the release records: each file of the cross-match's basis as it was read,
/// with its entry in the manifest.
fn rules(inputs: &Inputs<'_>) -> (Vec<ReleaseFile>, Vec<RecordedFile>) {
    let basis = &inputs.crossed.basis;
    let mut files = Vec::new();
    let mut recorded = Vec::new();
    for name in RULES {
        let bytes = basis.file(name).to_vec();
        let path = keys::rules(name);
        recorded.push(RecordedFile {
            name: name.to_owned(),
            rule: (name == "ranking").then(|| basis.ranking.rule.clone()),
            path: path.clone(),
            sha256: sha256(&bytes),
        });
        files.push(ReleaseFile {
            path,
            bytes,
            target: Target::R2,
        });
    }
    (files, recorded)
}

/// A rules file's text, as the release row records it.
fn text(inputs: &Inputs<'_>, name: &str) -> String {
    String::from_utf8_lossy(inputs.crossed.basis.file(name)).into_owned()
}

/// What the rows of a release came to, beside the rows.
struct Rows {
    d1: D1Rows,
    objects: ObjectRows,
    cells: CellRows,
    grids: Vec<GridFiles>,
}

/// Every D1 row of release `number`, and the grids' files.
fn rows(inputs: &Inputs<'_>, number: u32) -> Result<Rows, String> {
    let crossed = inputs.crossed;
    let ranking = &crossed.basis.ranking;
    let release =
        Release::new(&crossed.cosmology).with_peculiar_velocity(ranking.peculiar_velocity_km_s);
    let objects = object_rows(number, &crossed.objects, inputs.definitions)?;
    let (issued, replaced) = ledger_rows(&crossed.next.ledger)?;
    let sources = source_rows(
        number,
        inputs.definitions,
        &objects.per_source,
        inputs.cache,
    )?;
    let (structures, bounds) = structure_rows(
        number,
        &crossed.basis.structures,
        &crossed.structures,
        &release,
    )?;
    let cells = cell_rows(
        number,
        &crossed.objects,
        inputs.definitions,
        &crossed.cosmology,
        &bounds,
    )?;
    let grids = inputs
        .grids
        .iter()
        .map(|(d, g)| grid_files(number, g, d, &release))
        .collect::<Result<Vec<_>, _>>()?;
    let source_ids: Vec<String> = inputs
        .definitions
        .iter()
        .map(|d| d.id.to_string())
        .collect();
    let row = ReleaseRow {
        number,
        built_at: inputs.built_at.clone(),
        cosmology: json(&crossed.cosmology)?,
        peculiar_velocity_km_s: ranking.peculiar_velocity_km_s,
        model_version: MODEL_VERSION.to_owned(),
        ingest_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version: SCHEMA_VERSION,
        matching_rules: text(inputs, "matching"),
        ranking_rule: text(inputs, "ranking"),
        ranking_rule_name: ranking.rule.clone(),
        sources: json(&source_ids)?,
        objects: objects.objects.len() as u64,
        structures: structures.len() as u64,
        grids: grids.len() as u64,
        tiles: tile_count(inputs) as u64,
        // Estimated with the widest integer here, then set to the estimate it is part of,
        // which therefore errs high by at most a few bytes.
        d1_estimated_bytes: i64::MAX.unsigned_abs(),
    };
    let mut d1 = D1Rows::default();
    d1.add(&sources.sources);
    d1.add(&sources.payloads);
    d1.add(&sources.normalizations);
    d1.add(&objects.objects);
    d1.add(&objects.entries);
    d1.add(&objects.measurements);
    d1.add(&objects.designations);
    d1.add(&objects.names);
    d1.add(&issued);
    d1.add(&replaced);
    d1.add(&structures);
    d1.add(&domain_rows(number, inputs.tiles));
    d1.add(&grids.iter().map(|g| g.row.clone()).collect::<Vec<_>>());
    d1.add(&cells.objects);
    d1.add(&cells.structures);
    let own = ReleaseRow::TABLE.estimated_bytes(&row.values());
    let row = ReleaseRow {
        d1_estimated_bytes: d1.estimate().estimated_bytes + own,
        ..row
    };
    d1.add(&[row]);
    Ok(Rows {
        d1,
        objects,
        cells,
        grids,
    })
}

fn tile_count(inputs: &Inputs<'_>) -> usize {
    inputs.tiles.domains.iter().map(|d| d.nodes.len()).sum()
}

/// Assembles release `number` from `inputs` and checks it.
///
/// # Errors
///
/// Fails when a rules file cannot be read or a row cannot be made.
pub fn assemble(inputs: &Inputs<'_>, number: u32) -> Result<Assembled, String> {
    let (rule_files, recorded) = rules(inputs);
    let rows = rows(inputs, number)?;
    let mut estimate = rows.d1.estimate();
    // The release's row records the estimate it is part of, computed with its widest value.
    estimate.estimated_bytes = rows
        .d1
        .table(ReleaseRow::TABLE.name)
        .first()
        .and_then(|r| r.last())
        .and_then(|v| match v {
            SqlValue::Integer(n) => u64::try_from(*n).ok(),
            _ => None,
        })
        .unwrap_or(estimate.estimated_bytes);
    let failed = check_all(&Subject {
        d1: &rows.d1,
        objects: &inputs.crossed.objects,
        tiles: inputs.tiles,
        definitions: inputs.definitions,
        structures: &inputs.crossed.basis.structures,
        entries: &rows.objects.per_source,
        cache: inputs.cache,
    });
    let mut files = rule_files;
    for d in &inputs.tiles.domains {
        let name = name_of(d.domain);
        files.push(ReleaseFile {
            path: keys::tile_manifest(name),
            bytes: d.json.clone(),
            target: Target::R2,
        });
        for node in &d.nodes {
            files.push(ReleaseFile {
                path: keys::tile(name, &node.address),
                bytes: node.tile.clone(),
                target: Target::R2,
            });
        }
    }
    let d1_files = rows.d1.files();
    for g in &rows.grids {
        for (path, bytes) in &g.files {
            files.push(ReleaseFile {
                path: path.clone(),
                bytes: bytes.clone(),
                target: Target::R2,
            });
        }
    }
    for (path, bytes) in d1_files {
        files.push(ReleaseFile {
            path,
            bytes,
            target: Target::D1,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = manifest_of(inputs, number, recorded, &rows, estimate, &files);
    Ok(Assembled {
        files,
        manifest,
        failed,
    })
}

/// The manifest of release `number`, listing `files`.
fn manifest_of(
    inputs: &Inputs<'_>,
    number: u32,
    recorded: Vec<RecordedFile>,
    rows: &Rows,
    estimate: D1Estimate,
    files: &[ReleaseFile],
) -> ReleaseManifest {
    let crossed = inputs.crossed;
    let objects = &rows.objects;
    let placed = objects.objects.iter().filter(|o| o.placed).count();
    let assigned = &crossed.assigned;
    ReleaseManifest {
        format: FORMAT.to_owned(),
        manifest_version: MANIFEST_VERSION,
        release: number,
        built_at: inputs.built_at.clone(),
        cosmology: crossed.cosmology.clone(),
        peculiar_velocity_km_s: crossed.basis.ranking.peculiar_velocity_km_s,
        versions: Versions {
            univ_model: MODEL_VERSION.to_owned(),
            univ_ingest: env!("CARGO_PKG_VERSION").to_owned(),
            d1_schema: SCHEMA_VERSION,
            tile_encoding: univ_model::tile::VERSION,
            tile_manifest: TILE_MANIFEST_VERSION,
        },
        rules: recorded,
        cell_index: cell_index(&rows.cells),
        sources: inputs
            .definitions
            .iter()
            .map(|d| ManifestSource {
                id: d.id.to_string(),
                redistribution: d.redistribution.status.as_str().to_owned(),
                review: d.redistribution.review.as_str().to_owned(),
                entries: objects.per_source.get(&d.id).copied().unwrap_or(0),
            })
            .collect(),
        counts: Counts {
            objects: objects.objects.len() as u64,
            placed: placed as u64,
            entries: objects.entries.len() as u64,
            structures: crossed.basis.structures.len() as u64,
            structures_indexed: rows.cells.structures_indexed,
            grids: rows.grids.len() as u64,
            tiles: tile_count(inputs) as u64,
            object_cells: rows.cells.objects.len() as u64,
            structure_cells: rows.cells.structures.len() as u64,
        },
        ids: Ids {
            issued_first: assigned.issued.map(|r| r.first.get()),
            issued_last: assigned.issued.map(|r| r.last.get()),
            replaced: assigned.replaced.len() as u64,
        },
        d1: estimate,
        files: files
            .iter()
            .map(|f| ManifestFile {
                path: f.path.clone(),
                target: f.target,
                key: (f.target == Target::R2).then(|| keys::key(number, &f.path)),
                bytes: f.bytes.len() as u64,
                sha256: sha256(&f.bytes),
            })
            .collect(),
    }
}

/// The cell index's scheme, as the manifest records it.
fn cell_index(cells: &CellRows) -> CellIndex {
    CellIndex {
        scheme: "uniform grid of cubes anchored at the origin, cell = floor(coordinate / (edge \
                 * 2^level)); objects at level 0 of their scale domain, structures at the \
                 finest level their box meets at most structure_span cells a side"
            .to_owned(),
        frame: "ICRS equatorial Cartesian, origin the Solar System barycentre".to_owned(),
        spaces: SPACES
            .iter()
            .map(|s| CellSpaceInfo {
                name: s.name().to_owned(),
                unit: s.unit().to_owned(),
                edge: s.edge(),
            })
            .collect(),
        structure_span: STRUCTURE_SPAN,
        max_structure_level: MAX_STRUCTURE_LEVEL,
        not_indexed: cells
            .not_indexed
            .iter()
            .map(|(structure, reason)| NotIndexed {
                structure: structure.clone(),
                reason: reason.clone(),
            })
            .collect(),
    }
}

/// Writes `files` and then `manifest` into the new directory `dir`.
///
/// # Errors
///
/// Fails, as the manifest check, when a directory or file cannot be written.
pub fn write(dir: &Path, files: &[ReleaseFile], manifest: &ReleaseManifest) -> Result<(), Failed> {
    let fail = |path: &Path, e: &dyn std::fmt::Display| Failed {
        check: MANIFEST,
        cause: format!("{} cannot be written: {e}", path.display()),
    };
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| fail(dir, &e))?;
    }
    for f in files {
        let path = dir.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail(parent, &e))?;
        }
        std::fs::write(&path, &f.bytes).map_err(|e| fail(&path, &e))?;
    }
    let path = dir.join(keys::MANIFEST);
    crate::yaml::write(&path, manifest).map_err(|e| fail(&path, &e))
}

/// The manifest of release `number` as `text`, read from `origin`, checked to be of that
/// release.
///
/// # Errors
///
/// Fails, naming where it came from and the field, when it does not parse as a manifest or is
/// another release's.
pub fn parse_manifest(
    origin: &Path,
    text: &str,
    number: u32,
) -> Result<ReleaseManifest, FileError> {
    parse_checked(origin, text, |m: &ReleaseManifest| {
        if m.release == number {
            Ok(())
        } else {
            Err((
                "release".to_owned(),
                format!("is {}, not {number}", m.release),
            ))
        }
    })
}

/// The manifest of release `number` in its release directory `dir`, with its bytes as read.
///
/// # Errors
///
/// As [`parse_manifest`], and when the file cannot be read.
pub fn read_manifest(dir: &Path, number: u32) -> Result<(ReleaseManifest, Vec<u8>), FileError> {
    let path = dir.join(keys::MANIFEST);
    let bytes = std::fs::read(&path).map_err(|e| FileError::new(&path, "(file)", e.to_string()))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|e| FileError::new(&path, "(file)", e.to_string()))?;
    let manifest = parse_manifest(&path, text, number)?;
    Ok((manifest, bytes))
}

/// Reads every file the manifest lists back from `dir` and checks its size and SHA-256, and
/// that the directory holds no other file.
///
/// # Errors
///
/// Fails, as the manifest check, naming the first file that differs.
pub fn verify(dir: &Path, manifest: &ReleaseManifest) -> Result<(), Failed> {
    let fail = |cause: String| Failed {
        check: MANIFEST,
        cause,
    };
    for f in &manifest.files {
        let bytes =
            std::fs::read(dir.join(&f.path)).map_err(|e| fail(format!("{}: {e}", f.path)))?;
        if bytes.len() as u64 != f.bytes || sha256(&bytes) != f.sha256 {
            return Err(fail(format!("{} does not match its SHA-256", f.path)));
        }
    }
    let on_disk = list(dir, dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
    let listed = manifest.files.len() + 1;
    if on_disk.len() != listed {
        return Err(fail(format!(
            "the directory holds {} files, the manifest lists {listed} with itself",
            on_disk.len()
        )));
    }
    Ok(())
}

/// Every file under `dir`, by its path relative to `root`, sorted.
///
/// # Errors
///
/// Fails when a directory cannot be read.
pub fn list(root: &Path, dir: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(list(root, &path)?);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    Ok(out)
}
