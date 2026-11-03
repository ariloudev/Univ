//! The Local Void outlined on the real CF4++ density grid and tested against the real 2MRS,
//! 6dFGS DR3 and SDSS DR17 galaxies, normalized from their cached chunks, or from the recorded
//! fixtures (a few rows each) where no chunks are cached. Run by hand; it prints the numbers:
//!
//! `cargo test --release -p univ-ingest --test outlines_real -- --ignored --nocapture`
//!
//! `UNIV_INGEST_CACHE` names the cache holding `cf4pp-density/` (default: the workspace's
//! `.cache/univ-ingest`), and `UNIV_INGEST_CATALOGUES` the cache holding the catalogues' chunks
//! (default: the same).

mod common;

use std::path::PathBuf;

use univ_ingest::grids::{Window, read_cf4pp};
use univ_ingest::normalize::normalize;
use univ_ingest::outlines::{GridSky, Placed, compare, on_grid, place, region, scalar};
use univ_ingest::source::{Access, SourceDefinition, read_all as read_sources};
use univ_ingest::structures::{Structure, read_all};
use univ_model::entry::CatalogueEntry;
use univ_model::grid::{Axis, GridGeometry};
use univ_model::ids::SourceId;
use univ_model::redshift::{CmbCorrection, DipoleModel};

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn env_dir(name: &str, default: PathBuf) -> PathBuf {
    std::env::var_os(name).map_or(default, PathBuf::from)
}

/// The cached chunks of `definition`, or its recorded fixture.
fn tables(definition: &SourceDefinition) -> Vec<PathBuf> {
    let cache = common::cache_dir();
    let dir = env_dir("UNIV_INGEST_CATALOGUES", cache).join(definition.id.as_str());
    let mut chunks: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                // The row chunks, chunk-NNNN-*, not the answer to a count query.
                .filter(|p| {
                    p.file_name().is_some_and(|n| {
                        n.to_string_lossy()
                            .strip_prefix("chunk-")
                            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    chunks.sort();
    if !chunks.is_empty() {
        return chunks;
    }
    let extension = match definition.access {
        Access::VizierTsv { .. } => "tsv",
        _ => "csv",
    };
    vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(&definition.provider)
            .join(format!("{}.{extension}", definition.id)),
    ]
}

/// From `start`, steps to the lowest of the 26 neighbours until none is lower: the grid's own
/// minimum that the earlier Univ work outlined.
fn downhill(g: &GridGeometry, values: &[f64], mut cell: [usize; 3]) -> [usize; 3] {
    let at = |c: [usize; 3]| g.sample_index(c).map_or(f64::INFINITY, |i| values[i]);
    loop {
        let mut best = cell;
        for dx in 0..3 {
            for dy in 0..3 {
                for dz in 0..3 {
                    let n = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
                    if n.contains(&0) {
                        continue;
                    }
                    let n = n.map(|i| i - 1);
                    if at(n) < at(best) {
                        best = n;
                    }
                }
            }
        }
        if best == cell {
            return cell;
        }
        cell = best;
    }
}

#[test]
#[ignore = "reads the whole cached CF4++ grid and catalogue chunks; run by hand"]
fn the_local_void_outline_on_the_real_cf4pp_grid_and_catalogues() {
    let cache = common::cache_dir();
    let grid = read_cf4pp(
        &cache.join("cf4pp-density/new_CF4pp_mean_std_grids.npz"),
        Window::Whole,
    )
    .unwrap()
    .grid;
    let local: Structure = read_all(&workspace().join("structures"))
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Local Void")
        .unwrap();
    let record = local.outline.as_ref().unwrap();
    let definitions = read_sources(&workspace().join("sources")).unwrap();
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let normalized: Vec<(SourceId, Vec<CatalogueEntry>)> = record
        .test
        .catalogues
        .iter()
        .map(|id| {
            let definition = definitions.iter().find(|d| &d.id == id).unwrap();
            let files = tables(definition);
            let done = normalize(definition, &files, &correction).unwrap();
            eprintln!(
                "{id}: {} entries from {} file(s), {} left out",
                done.entries.len(),
                files.len(),
                done.report.left_out_total()
            );
            (id.clone(), done.entries)
        })
        .collect();
    let catalogues: Vec<(SourceId, &[CatalogueEntry])> = normalized
        .iter()
        .map(|(id, e)| (id.clone(), &e[..]))
        .collect();

    let result = place(&local, &grid, &catalogues).unwrap();
    let seed = result.seed.as_ref().unwrap();
    eprintln!(
        "Local Void: seed {} at {:?} h^-1 Mpc, cell {:?}, delta {:.3}; {} cells",
        seed.name, seed.at, seed.cell, seed.value, result.cells
    );
    for c in &result.catalogues {
        eprintln!("  {c:?}");
    }
    match &result.placed {
        Placed::Outline { .. } => eprintln!("  admitted as an outline"),
        Placed::Point { reason, .. } => eprintln!("  published as a point: {reason}"),
    }
    // Lacerta-2.4, [+1650, -700, +1650] km/s, is cell (66, 63, 66) of the grid.
    assert_eq!(seed.cell, [66, 63, 66]);

    // Diagnostic, not the record: the region below the threshold around the grid's own minimum
    // downhill of the published position, the region the earlier Univ work outlined.
    let values = scalar(&grid, &record.field).unwrap();
    let g = &grid.geometry;
    let start = g
        .cell_at(on_grid(local.position.as_ref().unwrap(), g).unwrap())
        .unwrap();
    let minimum = downhill(g, values, start);
    let centre = g.centre(minimum);
    let moved = Axis::XYZ
        .map(|a| centre[a.index()] - seed.at[a.index()])
        .iter()
        .map(|d| d * d)
        .sum::<f64>()
        .sqrt();
    let around = region(g, values, minimum, record.threshold.value(), record.side);
    eprintln!(
        "diagnostic: minimum downhill at cell {minimum:?}, {centre:?} h^-1 Mpc, delta {:.3}, \
         {moved:.1} h^-1 Mpc from the seed; {} cells below {}, touching the grid edge: {}",
        values[g.sample_index(minimum).unwrap()],
        around.cells.len(),
        record.threshold.value(),
        around.touches_edge
    );
    let sky = GridSky::new(g);
    for (id, entries) in &catalogues {
        let c = compare(&sky, &around, id, Some(entries), &record.test);
        eprintln!("  {c:?}");
    }
}
