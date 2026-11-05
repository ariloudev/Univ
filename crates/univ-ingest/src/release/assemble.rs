//! A release written into its directory in one pass: its rules, tiles and volume grids to the
//! paths their R2 keys name, its D1 rows streamed into SQL files as each object is turned into
//! rows, then checked, with the manifest of every file it wrote. Nothing holds a table's rows,
//! a statement beyond the one being written, or a copy of a tile or a grid's samples.

use std::collections::BTreeMap;
use std::path::Path;

use univ_model::derive::{MODEL_VERSION, Release};
use univ_model::ids::SourceId;
use univ_model::position::{BARYCENTRE, ICRS_CARTESIAN_NAME};
use univ_model::redshift::CmbCorrection;
use univ_model::release::cells::{MAX_STRUCTURE_LEVEL, SPACES, STRUCTURE_SPAN};
use univ_model::release::manifest::{
    CellIndex, CellSpaceInfo, Counts, D1Estimate, FORMAT, Ids, MANIFEST_VERSION, ManifestSource,
    NotIndexed, RELEASE_BUDGET_BYTES, RecordedFile, ReleaseManifest, ShardManifest, Versions,
};
use univ_model::release::rows::{GridRow, ObjectCellRow, ReleaseRow};
use univ_model::release::shards::Database;
use univ_model::release::{Row, SCHEMA_VERSION, keys};

use super::check::D1_KEYS;
use super::check::{Failed, Subject, check_all, d1_derived, d1_size};
use super::d1::{D1Writer, PartSink};
use super::entries::Numbering;
use super::files::DirSink;
use super::objects::{ObjectCounts, ObjectWriter, json, ledger_rows, snake};
use super::outline::Outlined;
use super::sharded::{Sharding, repeated_id, shard_manifests, write_shard_index};
use super::terms::{SourceIndex, Terms};
use super::{
    CellRows, Inputs, Placing, StructureBounds, TestCatalogues, coverages, domain_rows, grid_files,
    index_rows, object_cells, outlines_on, placements, read_grid, source_detail_rows, source_rows,
    structure_detail_rows, structure_rows,
};
use crate::crossmatch::RULES;
use crate::source::SourceDefinition;
use crate::tiles::{MANIFEST_VERSION as TILE_MANIFEST_VERSION, name_of, sha256};
use crate::watch::Watch;

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
    /// The catalogue database's estimate.
    estimate: D1Estimate,
    shards: Vec<ShardManifest>,
    failed: Vec<Failed>,
    objects: ObjectCounts,
    cells: CellRows,
    /// The placed objects, each with its row in the cell index.
    object_cells: u64,
    structures: u64,
}

/// Writes every D1 row of release `number` into `sink`, the grids' rows given, under
/// `release`, with the sources numbered by `sources`: both made once for the build. The
/// objects are written to the object shards planned for them, the rest to the catalogue.
fn d1_rows(
    inputs: &Inputs<'_>,
    number: u32,
    sink: &mut DirSink<'_>,
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
    let repeated = repeated_id(&crossed.objects);
    let cells = object_cells(&crossed.objects, inputs.definitions, &crossed.cosmology)?;
    let sharding = Sharding {
        definitions: inputs.definitions,
        cosmology: &crossed.cosmology,
        cells: &cells,
    };
    let budget = crossed.basis.config.shard_budget();
    let packing = sharding.plan(&crossed.objects, &numbering, budget, watch)?;
    let mut writer = D1Writer::new(sink, packing.count())?;
    let mut objects = ObjectWriter::new(numbering, inputs.definitions, &crossed.cosmology);
    sharding.write(&crossed.objects, &mut objects, &mut writer, &packing, watch)?;
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
    writer.add_all(&sources.terms)?;
    writer.add_all(chains.rows())?;
    let (issued, replaced) = ledger_rows(&crossed.next.ledger)?;
    writer.add_all(&issued)?;
    writer.add_all(&replaced)?;
    let (structures, bounds) = write_structures(&mut writer, inputs, number, release, grids)?;
    writer.add_all(&domain_rows(number, inputs.tiles))?;
    writer.add_all(&grids.rows)?;
    drop(cells);
    let cells = index_rows(number, &bounds)?;
    writer.add_all(&cells.structures)?;
    write_shard_index(&mut writer, number, &packing, &mut terms)?;
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
    let total = writer.estimated_bytes_in(Database::Catalogue)
        + ReleaseRow::TABLE.estimated_bytes(&row.values());
    writer.add(&ReleaseRow {
        d1_estimated_bytes: total,
        ..row
    })?;
    let object_cells = writer.rows(ObjectCellRow::TABLE.name);
    let written = writer.finish()?;
    let mut estimate = written.estimate;
    // The release's row records the estimate it is part of, computed with its widest value.
    estimate.estimated_bytes = total;
    let shards = shard_manifests(&packing, written.shards);
    let mut failed = written.failed;
    failed.extend(repeated.map(|id| Failed {
        check: D1_KEYS,
        cause: format!(
            "table objects: object id {id} is given to two objects, whose rows repeat its key \
             (release = {number}, id = {id})"
        ),
    }));
    failed.extend(sizes(&estimate, &shards, budget));
    failed.extend(d1_derived(
        counts.not_kept,
        counts.first_not_kept.as_deref(),
    ));
    Ok(D1Done {
        estimate,
        shards,
        failed,
        objects: counts,
        cells,
        object_cells,
        structures,
    })
}

/// The size checks of each database: the catalogue's `estimate` within a release's budget,
/// each of the `shards` within `budget`.
fn sizes(estimate: &D1Estimate, shards: &[ShardManifest], budget: u64) -> Vec<Failed> {
    let catalogue = d1_size(Database::Catalogue, estimate, RELEASE_BUDGET_BYTES);
    let each = shards
        .iter()
        .filter_map(|s| d1_size(Database::Shard(s.index), &s.d1, budget));
    catalogue.into_iter().chain(each).collect()
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
    let placing = Placing {
        release,
        correction: &correction,
        dipole: snake(&crossed.dipole),
    };
    let placed = placements(
        &crossed.basis.structures,
        &crossed.structures,
        &placing,
        &grids.outlines,
    )?;
    let (structures, bounds) = structure_rows(number, &placed)?;
    writer.add_all(&structures)?;
    writer.add_all(&structure_detail_rows(
        number,
        &placed,
        &grids.outlines,
        release,
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
    // Gathered once, for every grid's outline tests; empty when the release holds no grid.
    let catalogues = if inputs.grids.is_empty() {
        TestCatalogues::default()
    } else {
        TestCatalogues::new(&crossed.basis.structures, &crossed.objects)
    };
    for (definition, files) in inputs.grids {
        sink.watch.check()?;
        let grid = read_grid(definition, files)?;
        let mut g = grid_files(number, &grid, definition, release)?;
        grids.outlines.extend(outlines_on(
            &grid,
            &crossed.basis.structures,
            &crossed.objects,
            &catalogues,
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

/// Writes release `number` of `inputs` into the new directory `dir` and checks it, telling
/// `watch` as each of its phases is done: the rules, the tiles, the volume grids, the D1 rows and
/// the checks. Each file written, each grid and every
/// [`super::sharded::OBJECTS_BETWEEN_CHECKS`] objects is a
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
    let d1 = d1_rows(inputs, number, &mut sink, &grids, &release, sources, watch)?;
    let sql_files = sink.files.iter().filter(|f| f.key.is_none()).count();
    watch.step(format!(
        "D1 rows: {} rows in {sql_files} SQL files, in the catalogue database and {} object \
         shard(s)",
        std::iter::once(&d1.estimate)
            .chain(d1.shards.iter().map(|s| &s.d1))
            .flat_map(|e| &e.tables)
            .map(|t| t.rows)
            .sum::<u64>(),
        d1.shards.len()
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
        sources: manifest_sources(inputs.definitions, &d1.objects.per_source),
        counts: Counts {
            objects: d1.objects.objects,
            placed: d1.objects.placed,
            entries: d1.objects.entries,
            structures: d1.structures,
            structures_indexed: d1.cells.structures_indexed,
            grids: grids.rows.len() as u64,
            tiles: tile_count(inputs) as u64,
            object_cells: d1.object_cells,
            structure_cells: d1.cells.structures.len() as u64,
        },
        ids: Ids {
            issued_first: assigned.issued.map(|r| r.first.get()),
            issued_last: assigned.issued.map(|r| r.last.get()),
            replaced: assigned.replaced.len() as u64,
        },
        d1: d1.estimate,
        shards: d1.shards,
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

/// Each of `definitions` as the release manifest lists it, with the entries the release holds
/// from it among `per_source`.
fn manifest_sources(
    definitions: &[SourceDefinition],
    per_source: &BTreeMap<SourceId, u64>,
) -> Vec<ManifestSource> {
    definitions
        .iter()
        .map(|d| ManifestSource {
            id: d.id.to_string(),
            redistribution: d.redistribution.status.as_str().to_owned(),
            review: d.redistribution.review.as_str().to_owned(),
            non_commercial: d.terms.non_commercial.clone(),
            permission: d.redistribution.permission.clone(),
            entries: per_source.get(&d.id).copied().unwrap_or(0),
        })
        .collect()
}
