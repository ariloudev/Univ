//! A release written into its directory in one pass: its rules, tiles and volume grids to the
//! paths their R2 keys name, its D1 rows streamed into SQL files as each object is turned into
//! rows, then checked, with the manifest of every file it wrote. Nothing holds a table's rows,
//! a statement beyond the one being written, or a copy of a tile or a grid's samples.

use std::collections::BTreeMap;
use std::path::Path;

use univ_model::derive::{MODEL_VERSION, Release};
use univ_model::position::{BARYCENTRE, ICRS_CARTESIAN_NAME};
use univ_model::redshift::CmbCorrection;
use univ_model::release::cells::{MAX_STRUCTURE_LEVEL, SPACES, STRUCTURE_SPAN};
use univ_model::release::manifest::{
    CellIndex, CellSpaceInfo, Counts, D1Estimate, FORMAT, Ids, MANIFEST_VERSION, ManifestSource,
    NotIndexed, RecordedFile, ReleaseManifest, Versions,
};
use univ_model::release::rows::{GridRow, ObjectCellRow, ReleaseRow};
use univ_model::release::{Row, SCHEMA_VERSION, keys};

use super::check::{Failed, Subject, check_all, d1_derived, d1_size};
use super::d1::{D1Writer, PartSink};
use super::entries::Numbering;
use super::files::DirSink;
use super::objects::{ObjectCounts, ObjectWriter, json, ledger_rows, snake};
use super::outline::Outlined;
use super::terms::{SourceIndex, Terms};
use super::{
    CellRows, Inputs, Placing, StructureBounds, cell_rows, coverages, domain_rows, grid_files,
    outlines_on, read_grid, source_detail_rows, source_rows, structure_detail_rows, structure_rows,
};
use crate::crossmatch::RULES;
use crate::tiles::{MANIFEST_VERSION as TILE_MANIFEST_VERSION, name_of, sha256};
use crate::watch::Watch;

/// Objects turned into rows between two looks at the build's stop flag.
const OBJECTS_BETWEEN_CHECKS: usize = 10_000;

/// A release written into its directory, with the failures of its checks.
pub struct Written {
    pub manifest: ReleaseManifest,
    pub failed: Vec<Failed>,
}

/// Writes each rules file of the cross-match's basis as it was read, giving its entry in the
/// manifest.
fn rules(inputs: &Inputs<'_>, sink: &mut DirSink<'_>) -> Result<Vec<RecordedFile>, String> {
    let basis = &inputs.crossed.basis;
    let mut recorded = Vec::new();
    for name in RULES {
        let bytes = basis.file(name);
        let path = keys::rules(name);
        recorded.push(RecordedFile {
            name: name.to_owned(),
            rule: (name == "ranking").then(|| basis.ranking.rule.clone()),
            path: path.clone(),
            sha256: sha256(bytes),
        });
        sink.put(path, bytes)?;
    }
    Ok(recorded)
}

/// A rules file's text, as the release row records it.
fn text(inputs: &Inputs<'_>, name: &str) -> String {
    String::from_utf8_lossy(inputs.crossed.basis.file(name)).into_owned()
}

fn tile_count(inputs: &Inputs<'_>) -> usize {
    inputs.tiles.domains.iter().map(|d| d.nodes.len()).sum()
}

/// What the D1 rows came to.
struct D1Done {
    estimate: D1Estimate,
    failed: Vec<Failed>,
    objects: ObjectCounts,
    cells: CellRows,
    structures: u64,
}

/// Writes every D1 row of release `number` through `writer`, the grids' rows given, under
/// `release`, with the sources numbered by `sources`: both made once for the build.
fn d1_rows(
    inputs: &Inputs<'_>,
    number: u32,
    mut writer: D1Writer<'_>,
    grids: &WrittenGrids,
    release: &Release,
    sources: SourceIndex,
    watch: Watch<'_>,
) -> Result<D1Done, String> {
    let crossed = inputs.crossed;
    let ranking = &crossed.basis.ranking;
    let numbering = Numbering {
        release: number,
        terms: Terms::default(),
        sources,
    };
    let objects = write_objects(inputs, numbering, &mut writer, watch)?;
    let ObjectWriter {
        numbering: Numbering { mut terms, .. },
        chains,
        counts,
        ..
    } = objects;
    let sources = source_rows(number, inputs.definitions, &counts.per_source, inputs.cache)?;
    writer.add_all(&sources.sources)?;
    writer.add_all(&sources.payloads)?;
    writer.add_all(&sources.normalizations)?;
    writer.add_all(chains.rows())?;
    let (issued, replaced) = ledger_rows(&crossed.next.ledger)?;
    writer.add_all(&issued)?;
    writer.add_all(&replaced)?;
    let (structures, bounds) = write_structures(&mut writer, inputs, number, release, grids)?;
    writer.add_all(&domain_rows(number, inputs.tiles))?;
    writer.add_all(&grids.rows)?;
    let cells = cell_rows(
        number,
        &crossed.objects,
        inputs.definitions,
        &crossed.cosmology,
        &bounds,
    )?;
    for c in &cells.objects {
        writer.add(&ObjectCellRow {
            release: number,
            domain: terms.id(c.domain),
            ix: c.ix,
            iy: c.iy,
            iz: c.iz,
            object_id: c.object_id,
        })?;
    }
    writer.add_all(&cells.structures)?;
    writer.add_all(&terms.rows(number))?;
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
        objects: counts.objects,
        structures,
        grids: grids.rows.len() as u64,
        tiles: tile_count(inputs) as u64,
        // Estimated with the widest integer here, then set to the estimate it is part of,
        // which therefore errs high by at most a few bytes.
        d1_estimated_bytes: i64::MAX.unsigned_abs(),
    };
    let total = writer.estimated_bytes() + ReleaseRow::TABLE.estimated_bytes(&row.values());
    writer.add(&ReleaseRow {
        d1_estimated_bytes: total,
        ..row
    })?;
    let written = writer.finish()?;
    let mut estimate = written.estimate;
    // The release's row records the estimate it is part of, computed with its widest value.
    estimate.estimated_bytes = total;
    let mut failed = written.failed;
    failed.extend(d1_size(&estimate));
    failed.extend(d1_derived(
        counts.not_kept,
        counts.first_not_kept.as_deref(),
    ));
    Ok(D1Done {
        estimate,
        failed,
        objects: counts,
        cells,
        structures,
    })
}

/// Writes the `source_details`, `structures` and `structure_details` rows of release `number`
/// through `writer`, its structures placed under `release` and the run's dipole, with the
/// `outline` columns the grids gave; it gives how many structures it wrote and their boxes.
fn write_structures(
    writer: &mut D1Writer<'_>,
    inputs: &Inputs<'_>,
    number: u32,
    release: &Release,
    grids: &WrittenGrids,
) -> Result<(u64, Vec<StructureBounds>), String> {
    let crossed = inputs.crossed;
    let covered = coverages(&crossed.objects, inputs.definitions);
    writer.add_all(&source_detail_rows(number, inputs.definitions, &covered)?)?;
    let correction = CmbCorrection::new(crossed.dipole);
    let (structures, bounds) = structure_rows(
        number,
        &crossed.basis.structures,
        &crossed.structures,
        &grids.outlines,
        release,
        &correction,
    )?;
    writer.add_all(&structures)?;
    let placing = Placing {
        release,
        correction: &correction,
        dipole: snake(&crossed.dipole),
    };
    writer.add_all(&structure_detail_rows(
        number,
        &crossed.basis.structures,
        &crossed.structures,
        &placing,
        &grids.outlines,
    )?)?;
    Ok((structures.len() as u64, bounds))
}

/// The volume grids written: their rows, and the `outline` column of each structure outlined
/// on one of them, by its id.
struct WrittenGrids {
    rows: Vec<GridRow>,
    outlines: BTreeMap<String, Outlined>,
}

/// Writes each volume grid's files, one grid at a time: read, written, its structures outlined
/// and tested, and let go. It gives each grid's row, its source numbered by `sources`.
fn write_grids(
    inputs: &Inputs<'_>,
    number: u32,
    sink: &mut DirSink<'_>,
    release: &Release,
    sources: &mut SourceIndex,
) -> Result<WrittenGrids, String> {
    sink.watch.check()?;
    let crossed = inputs.crossed;
    let mut grids = WrittenGrids {
        rows: Vec::with_capacity(inputs.grids.len()),
        outlines: BTreeMap::new(),
    };
    let mut written = 0;
    for (definition, files) in inputs.grids {
        sink.watch.check()?;
        let grid = read_grid(definition, files)?;
        let mut g = grid_files(number, &grid, definition, release)?;
        grids.outlines.extend(outlines_on(
            &grid,
            &crossed.basis.structures,
            &crossed.objects,
            release,
        )?);
        drop(grid);
        for (path, bytes) in &g.files {
            sink.put(path.clone(), bytes)?;
        }
        written += g.files.len();
        g.row.source = sources.n(&definition.id);
        grids.rows.push(g.row);
    }
    let tested = match grids.outlines.len() {
        0 => String::new(),
        n => format!(", {n} outlines tested"),
    };
    sink.watch.step(format!(
        "volume grids: {} grids in {written} files{tested}",
        grids.rows.len(),
    ));
    Ok(grids)
}

/// Turns every object into its rows through `writer`, looking at the build's stop flag every
/// [`OBJECTS_BETWEEN_CHECKS`] objects.
fn write_objects<'a>(
    inputs: &Inputs<'a>,
    numbering: Numbering,
    writer: &mut D1Writer<'_>,
    watch: Watch<'_>,
) -> Result<ObjectWriter<'a>, String> {
    let crossed = inputs.crossed;
    let mut objects = ObjectWriter::new(numbering, inputs.definitions, &crossed.cosmology);
    for (n, o) in crossed.objects.iter().enumerate() {
        if n % OBJECTS_BETWEEN_CHECKS == 0 {
            watch.check()?;
        }
        objects.write(writer, o)?;
    }
    Ok(objects)
}

/// Writes release `number` of `inputs` into the new directory `dir` and checks it, telling
/// `watch` as each of its phases is done: the rules, the tiles, the volume grids, the D1 rows and
/// the checks. Each file written, each grid and every [`OBJECTS_BETWEEN_CHECKS`] objects is a
/// safe point at which the build stops once the owner asks it to.
///
/// # Errors
///
/// Fails when a file cannot be read or written or a row cannot be made, or when the owner asked
/// the build to stop.
pub fn write_release(
    inputs: &Inputs<'_>,
    number: u32,
    dir: &Path,
    watch: Watch<'_>,
) -> Result<Written, String> {
    let mut sink = DirSink::new(dir, number, watch)?;
    let recorded = rules(inputs, &mut sink)?;
    watch.step(format!("rules: {} files", recorded.len()));
    for d in &inputs.tiles.domains {
        let name = name_of(d.domain);
        sink.put(keys::tile_manifest(name), &d.json)?;
        for node in &d.nodes {
            sink.put(keys::tile(name, &node.address), &node.tile)?;
        }
    }
    watch.step(format!(
        "tiles: {} tiles and {} manifests",
        tile_count(inputs),
        inputs.tiles.domains.len()
    ));
    let crossed = inputs.crossed;
    let release = Release::new(&crossed.cosmology)
        .with_peculiar_velocity(crossed.basis.ranking.peculiar_velocity_km_s);
    let mut sources = SourceIndex::new(inputs.definitions.iter().map(|d| &d.id));
    let grids = write_grids(inputs, number, &mut sink, &release, &mut sources)?;
    let d1 = d1_rows(
        inputs,
        number,
        D1Writer::new(&mut sink)?,
        &grids,
        &release,
        sources,
        watch,
    )?;
    let sql_files = sink.files.iter().filter(|f| f.key.is_none()).count();
    watch.step(format!(
        "D1 rows: {} rows in {sql_files} SQL files",
        d1.estimate.tables.iter().map(|t| t.rows).sum::<u64>()
    ));
    watch.check()?;
    let mut failed = check_all(&Subject {
        objects: &crossed.objects,
        tiles: inputs.tiles,
        definitions: inputs.definitions,
        structures: &crossed.basis.structures,
        entries: &d1.objects.per_source,
        cache: inputs.cache,
    });
    failed.extend(d1.failed.iter().cloned());
    watch.step(if failed.is_empty() {
        "checks: every check passed".to_owned()
    } else {
        format!("checks: {} failed", failed.len())
    });
    let mut files = sink.files;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let assigned = &crossed.assigned;
    let manifest = ReleaseManifest {
        format: FORMAT.to_owned(),
        manifest_version: MANIFEST_VERSION,
        release: number,
        built_at: inputs.built_at.clone(),
        cosmology: crossed.cosmology.clone(),
        cosmology_note: crossed.basis.config.cosmology_note.clone(),
        peculiar_velocity_km_s: crossed.basis.ranking.peculiar_velocity_km_s,
        versions: Versions {
            univ_model: MODEL_VERSION.to_owned(),
            univ_ingest: env!("CARGO_PKG_VERSION").to_owned(),
            d1_schema: SCHEMA_VERSION,
            tile_encoding: univ_model::tile::VERSION,
            tile_manifest: TILE_MANIFEST_VERSION,
        },
        rules: recorded,
        cell_index: cell_index(&d1.cells),
        sources: inputs
            .definitions
            .iter()
            .map(|d| ManifestSource {
                id: d.id.to_string(),
                redistribution: d.redistribution.status.as_str().to_owned(),
                review: d.redistribution.review.as_str().to_owned(),
                entries: d1.objects.per_source.get(&d.id).copied().unwrap_or(0),
            })
            .collect(),
        counts: Counts {
            objects: d1.objects.objects,
            placed: d1.objects.placed,
            entries: d1.objects.entries,
            structures: d1.structures,
            structures_indexed: d1.cells.structures_indexed,
            grids: grids.rows.len() as u64,
            tiles: tile_count(inputs) as u64,
            object_cells: d1.cells.objects.len() as u64,
            structure_cells: d1.cells.structures.len() as u64,
        },
        ids: Ids {
            issued_first: assigned.issued.map(|r| r.first.get()),
            issued_last: assigned.issued.map(|r| r.last.get()),
            replaced: assigned.replaced.len() as u64,
        },
        d1: d1.estimate,
        files,
    };
    Ok(Written { manifest, failed })
}

/// The cell index's scheme, as the manifest records it.
fn cell_index(cells: &CellRows) -> CellIndex {
    CellIndex {
        scheme: "uniform grid of cubes anchored at the origin, cell = floor(coordinate / (edge \
                 * 2^level)); objects at level 0 of their scale domain, structures at the \
                 finest level their box meets at most structure_span cells a side"
            .to_owned(),
        frame: format!("{ICRS_CARTESIAN_NAME}, origin the {BARYCENTRE}"),
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
