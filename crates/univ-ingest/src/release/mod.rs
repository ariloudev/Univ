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
//! The build checks the release in memory, writes it to `releases/.building-{n}` and checks
//! every file against the manifest; only then is the directory renamed to `releases/{n}` and the
//! state the cross-match made, carried in its result, installed as the state directory's
//! (`crate::state::State::install`): its `current` switched by one rename, which is what uses
//! the number up. A build that fails at any step removes what it wrote and leaves the state as it
//! was, so the next build gets the same number.
//!
//! A build stopped between the two renames leaves a `releases/{n}` the state does not record.
//! Publishing refuses such a release, and the next build names it and stops rather than repair
//! anything: it is removed by hand, or built over with the state that built it.

mod cells;
mod check;
mod config;
mod d1;
mod files;
mod grids;
mod meta;
mod objects;
mod place;

use std::fmt;
use std::path::{Path, PathBuf};

use univ_model::grid::Grid;
use univ_model::ids::SourceId;
use univ_model::release::ReleaseManifest;

pub use cells::{CellRows, cell_rows};
pub use check::{
    D1_FINITE, D1_KEYS, D1_SIZE, ENTRY_COUNTS, Failed, MANIFEST, REDISTRIBUTION,
    STRUCTURE_CITATIONS, Subject, TILE_OBJECTS, check_all, decimal_bytes, provisional,
};
pub use config::{FILE as CONFIG_FILE, ReleaseConfig};
pub use d1::{D1Rows, MAX_FILE_BYTES};
pub use files::{
    Assembled, RELEASES, ReleaseFile, assemble, list, parse_manifest, read_manifest, release_dir,
    verify,
};
pub use grids::{GridFiles, grid_files, is_grid, read as read_grid};
pub use meta::{SourceRows, StructureBounds, domain_rows, source_rows, structure_rows};
pub use objects::{ObjectRows, json, ledger_rows, object_rows, snake};
pub use place::{index_bounds, sky_region};

use crate::crossmatch::CrossMatched;
use crate::source::SourceDefinition;
use crate::state::{State, pending};
use crate::structures::Structure;
use crate::tiles::Built;

/// What a release is built from.
pub struct Inputs<'a> {
    pub crossed: &'a CrossMatched,
    pub tiles: &'a Built,
    /// The definitions of every source the release holds.
    pub definitions: &'a [SourceDefinition],
    /// The researched structures, each by its id.
    pub structures: &'a [(String, Structure)],
    /// The volume grids, each with its source's definition.
    pub grids: &'a [(SourceDefinition, Grid)],
    pub config: &'a ReleaseConfig,
    /// The repository's `rules/`, whose files the release records.
    pub rules: &'a Path,
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
    let assembled = assemble(inputs, number).map_err(fail)?;
    if !assembled.failed.is_empty() {
        return Err(BuildError::Checks {
            number,
            failed: assembled.failed,
        });
    }
    let building = out.join(RELEASES).join(format!(".building-{number}"));
    let written = files::write(&building, &assembled.files, &assembled.manifest)
        .and_then(|()| files::verify(&building, &assembled.manifest));
    if let Err(failed) = written {
        let _ = std::fs::remove_dir_all(&building);
        return Err(BuildError::Checks {
            number,
            failed: vec![failed],
        });
    }
    std::fs::rename(&building, &dir).map_err(|e| {
        let _ = std::fs::remove_dir_all(&building);
        fail(format!("{} cannot be made complete: {e}", dir.display()))
    })?;
    if let Err(problem) = record(state, &inputs.crossed.next, number) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(fail(problem));
    }
    // The copy the cross-match left; the state just installed is the one it made.
    let _ = std::fs::remove_dir_all(pending(state));
    Ok(BuiltRelease {
        number,
        dir,
        manifest: assembled.manifest,
        provisional: provisional(inputs.definitions),
    })
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
fn record(state: &Path, next: &State, number: u32) -> Result<(), String> {
    let last = next.ledger.releases().last().map(|r| r.name.as_str());
    if last != Some(number.to_string().as_str()) {
        return Err(format!(
            "the cross-match's state is not that of release {number}"
        ));
    }
    next.install(state)
        .map_err(|e| format!("the state cannot be recorded in {}: {e}", state.display()))
}
