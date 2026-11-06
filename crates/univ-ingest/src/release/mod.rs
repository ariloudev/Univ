//! Building a release (build-a-release): everything a release holds assembled on local disk,
//! checked, and only then made complete under the next release number.
//!
//! A release is written as `univ_model::release` lays it out: its D1 rows as SQL files under
//! `d1/`, and its R2 files (rules, tiles, tile manifests, volume grids) at the paths their keys
//! name below `releases/{n}/`, with `manifest.yaml` listing every other file and its SHA-256.
//!
//! # No number used up by a failed build (AC-90)
//!
//! The number is the next the id ledger gives, `n`, which the cross-match prepared its ids for.
//! The build writes the release to `releases/.building-{n}` as it makes its files, streaming
//! its D1 rows, checks it, and checks every file against the manifest; only then is the directory renamed to `releases/{n}` and the
//! state the cross-match made, carried in its result, installed as the state directory's
//! (`crate::state::State::install`): its `current` switched by one rename, which is what uses
//! the number up. A build that fails at any step removes what it wrote and leaves the state as it
//! was, so the next build gets the same number.
//!
//! A build stopped between the two renames leaves a `releases/{n}` the state does not record.
//! Publishing refuses such a release, and the next build names it and stops rather than repair
//! anything: it is removed by hand, or built over with the state that built it.

mod assemble;
mod cells;
mod check;
mod config;
mod coverage;
mod d1;
mod details;
mod entries;
mod files;
mod grids;
mod kept;
pub use d1::ROWS;
pub use kept::{Kept, NUMBERING};
mod meta;
mod objects;
mod outline;
mod packing;
pub use packing::LOOSE_BUCKETS;
mod place;
mod placement;
mod placing;
mod pointers;
pub mod reindex;
mod rescaled;
mod shapes;
mod sharded;
mod sky;
mod terms;

use std::fmt;
use std::path::{Path, PathBuf};

use univ_model::ids::SourceId;
use univ_model::release::ReleaseManifest;

pub use assemble::{Written, write_release};
pub use cells::{CellRows, DomainCell, ObjectCells, index_rows, object_cells};
pub use check::{
    D1_DERIVED, D1_FINITE, D1_KEYS, D1_SIZE, ENTRY_COUNTS, Failed, MANIFEST, REDISTRIBUTION,
    STRUCTURE_CITATIONS, Subject, TILE_OBJECTS, check_all, decimal_bytes, provisional, table_sizes,
};
pub use config::{FILE as CONFIG_FILE, ReleaseConfig};
pub use coverage::{Coverage, coverages, source_detail_rows};
pub use d1::Before;
pub use d1::{D1Writer, D1Written, InMemory, MAX_FILE_BYTES, PartSink, content_sha256};
pub use details::structure_detail_rows;
pub use files::{
    DirSink, RELEASES, list, parse_manifest, read_manifest, release_dir, verify, write_manifest,
};
pub use grids::{GridFiles, grid_files, is_grid, read as read_grid};
pub use meta::{SourceRows, StructureBounds, domain_rows, source_rows, structure_rows};
pub use objects::{ObjectCounts, ObjectWriter, json, ledger_rows, snake};
pub use outline::{
    Admitted, OUTLINE_CENTRE, Outlined, TestCatalogues, admitted, not_drawn, outlines_on,
    result_record,
};
pub use place::{SkyRanges, centre, index_bounds, ranges, sky_region};
pub use placement::{Placement, placements};
pub use rescaled::rescaled;
pub use shapes::{PlacedShape, Placing, frame_chain, placed_shape};
pub use sky::{COVERAGE_ORDER, REGION_ORDER, box_cells, cell as healpix_cell, cells_of};
pub use terms::{Chains, SourceIndex, Terms, variant_name};

use crate::crossmatch::CrossMatched;
use crate::source::SourceDefinition;
use crate::state::{State, pending};
use crate::tiles::Built;
use crate::watch::Watch;

/// The phases of a build its watch is told of: the rules, the tiles, the volume grids, the D1
/// rows, the checks, and the manifest with every file checked against it.
pub const PHASES: u64 = 6;

/// What a release is built from. Its rules, release configuration and researched structures
/// are those the cross-match worked under, which it carries (`crate::crossmatch::Basis`).
pub struct Inputs<'a> {
    pub crossed: &'a CrossMatched,
    pub tiles: &'a Built,
    /// The definitions of every source the release holds.
    pub definitions: &'a [SourceDefinition],
    /// The volume grids, each by its source's definition and fetched files, read one at a time
    /// as the release is written.
    pub grids: &'a [(SourceDefinition, Vec<PathBuf>)],
    /// The cache, which keeps each source's provenance.
    pub cache: &'a Path,
    /// The build time, RFC 3339 in UTC.
    pub built_at: String,
}

/// A release built and complete.
#[derive(Clone, Debug)]
pub struct BuiltRelease {
    pub number: u32,
    pub dir: PathBuf,
    pub manifest: ReleaseManifest,
    /// The sources whose redistribution review is provisional.
    pub provisional: Vec<SourceId>,
}

/// A build that failed: nothing was marked complete and no number was used up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// Checks of the release failed, each named with its cause.
    Checks { number: u32, failed: Vec<Failed> },
    /// The release could not be assembled, written or made complete.
    Release {
        number: Option<u32>,
        problem: String,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checks { number, failed } => {
                let causes: Vec<String> = failed.iter().map(ToString::to_string).collect();
                write!(
                    f,
                    "release {number} was not built, and its number is still free: {}",
                    causes.join("; ")
                )
            }
            Self::Release {
                number: Some(n),
                problem,
            } => write!(
                f,
                "release {n} was not built, and its number is still free: {problem}"
            ),
            Self::Release {
                number: None,
                problem,
            } => write!(f, "no release was built: {problem}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Builds the release of `inputs` under `out`, using up its number in the state directory
/// `state` only once it is complete.
///
/// # Errors
///
/// Fails, leaving the state and `out` as they were, when the state does not hold the pending
/// state of this release, its directory already exists, a check fails, or a file cannot be
/// written or checked.
pub fn build(inputs: &Inputs<'_>, state: &Path, out: &Path) -> Result<BuiltRelease, BuildError> {
    build_watched(inputs, state, out, Watch::unwatched())
}

/// [`build`], telling `watch` as each of its [`PHASES`] is done. Once the owner asks it to stop,
/// it stops at its next safe point, before the release is made complete: the directory it was
/// writing is removed, no number is used up and the state is left as it was.
///
/// # Errors
///
/// Fails as [`build`] does, or when the owner asked the build to stop.
pub fn build_watched(
    inputs: &Inputs<'_>,
    state: &Path,
    out: &Path,
    watch: Watch<'_>,
) -> Result<BuiltRelease, BuildError> {
    let number = inputs
        .crossed
        .release
        .parse::<u32>()
        .map_err(|_| BuildError::Release {
            number: None,
            problem: format!("the cross-match named release {:?}", inputs.crossed.release),
        })?;
    let fail = |problem: String| BuildError::Release {
        number: Some(number),
        problem,
    };
    let current = State::read(state).map_err(|e| fail(e.to_string()))?;
    let made = current.ledger.releases().len();
    if made + 1 != number as usize {
        return Err(fail(format!(
            "the state in {} records {made} release(s), so the next is {}, not {number}",
            state.display(),
            made + 1
        )));
    }
    let unrecorded = unrecorded(out, made);
    if !unrecorded.is_empty() {
        let dirs: Vec<String> = unrecorded.iter().map(|d| d.display().to_string()).collect();
        let exists = if dirs.len() == 1 { "exists" } else { "exist" };
        return Err(fail(format!(
            "{} already {exists}, but the state in {} records {made} release(s): a build \
             stopped before it recorded them, or another state built them. Nothing is \
             repaired: remove them, or build with the state that built them",
            dirs.join(", "),
            state.display()
        )));
    }
    let dir = release_dir(out, number);
    let building = out.join(RELEASES).join(format!(".building-{number}"));
    let rows = row_index(state, made, number).map_err(fail)?;
    let next_rows = crate::state::next_rows_dir(state);
    let before = Before {
        kept: &current.kept,
        rows: rows.as_deref(),
        next_rows: &next_rows,
    };
    // What a build that stops or fails leaves: its release directory and the next row index.
    let discard = || {
        let _ = std::fs::remove_dir_all(&building);
        let _ = std::fs::remove_dir_all(&next_rows);
    };
    let written = write_release(inputs, number, &building, &before, watch).map_err(|problem| {
        discard();
        fail(problem)
    })?;
    if let Err(stopped) = watch.check() {
        discard();
        return Err(fail(stopped));
    }
    if !written.failed.is_empty() {
        discard();
        return Err(BuildError::Checks {
            number,
            failed: written.failed,
        });
    }
    let checked = files::write_manifest(&building, &written.manifest)
        .and_then(|()| files::verify(&building, &written.manifest));
    if let Err(failed) = checked {
        discard();
        return Err(BuildError::Checks {
            number,
            failed: vec![failed],
        });
    }
    watch.step(format!(
        "manifest: {} files checked against their SHA-256",
        written.manifest.files.len()
    ));
    // The last safe point: from here the release is made complete and its number used up.
    if let Err(stopped) = watch.check() {
        discard();
        return Err(fail(stopped));
    }
    std::fs::rename(&building, &dir).map_err(|e| {
        discard();
        fail(format!("{} cannot be made complete: {e}", dir.display()))
    })?;
    if let Err(problem) = record(
        state,
        &inputs.crossed.next,
        (&written.kept, &next_rows),
        number,
    ) {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&next_rows);
        return Err(fail(problem));
    }
    // The copy the cross-match left; the state just installed is the one it made.
    let _ = std::fs::remove_dir_all(pending(state));
    Ok(BuiltRelease {
        number,
        dir,
        manifest: written.manifest,
        provisional: provisional(inputs.definitions),
    })
}

/// The row index of the last of the `made` releases the state directory `state` records, which
/// release `number` is compared with: none before the first.
fn row_index(state: &Path, made: usize, number: u32) -> Result<Option<PathBuf>, String> {
    let rows = crate::state::rows_dir(state).map_err(|e| e.to_string())?;
    if rows.is_none() && made > 0 {
        return Err(format!(
            "the state in {} holds no row index of release {made}, which release {number} \
             would be compared with: every row would be written again and none of release \
             {made}'s retired",
            state.display()
        ));
    }
    Ok(rows)
}

/// Each `releases/{k}` under `out` whose number is beyond the `made` releases a state records.
#[must_use]
pub fn unrecorded(out: &Path, made: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(out.join(RELEASES)) else {
        return Vec::new();
    };
    let mut found: Vec<(usize, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let k = e.file_name().to_str()?.parse::<usize>().ok()?;
            (k > made).then(|| (k, e.path()))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// Installs `next`, the state the cross-match made for release `number`, as the state
/// directory's: the one step that uses the number up.
fn record(
    state: &Path,
    next: &State,
    (kept, rows): (&Kept, &Path),
    number: u32,
) -> Result<(), String> {
    let last = next.ledger.releases().last().map(|r| r.name.as_str());
    if last != Some(number.to_string().as_str()) {
        return Err(format!(
            "the cross-match's state is not that of release {number}"
        ));
    }
    next.install(state, kept, rows)
        .map_err(|e| format!("the state cannot be recorded in {}: {e}", state.display()))
}
