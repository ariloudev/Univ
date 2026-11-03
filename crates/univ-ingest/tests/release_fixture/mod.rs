//! The fixture a release is built from in tests: the recorded answers of five `VizieR`
//! sources, normalized, with their provenance in a cache, the repository's researched
//! structures and rules, and a fresh state directory and build output.

#![allow(dead_code, reason = "each test file uses a part of the fixture")]

use std::path::Path;

use crate::common::{definition_of, normalized_fixture, source_id, workspace};
use tempfile::TempDir;
use univ_ingest::crossmatch::{Basis, CrossMatched, Inputs as MatchInputs, cross_match};
use univ_ingest::events::RecordingSink;
use univ_ingest::normalize::{NormalizationReport, Normalized};
use univ_ingest::provenance::Provenance;
use univ_ingest::release::{BuildError, BuiltRelease, Inputs, build};
use univ_ingest::source::SourceDefinition;
use univ_ingest::state::State;
use univ_ingest::structures::{Structure, read_all_named};
use univ_ingest::tiles::{Built, Context, TileRules, build as build_tiles};
use univ_model::derive::Release;
use univ_model::release::{ReleaseManifest, keys};

/// Sources each fetched in one query: Hipparcos stars, nearby galaxies and cosmic web galaxies.
pub const SOURCES: [&str; 5] = [
    "hipparcos-2007",
    "mcconnachie-2012",
    "updated-nearby-galaxies",
    "twomrs",
    "cosmicflows-4",
];

/// The sources' entries, definitions and provenance, the structures, and a state and output.
pub struct Fixture {
    pub definitions: Vec<SourceDefinition>,
    pub normalized: Vec<Normalized>,
    pub structures: Vec<(String, Structure)>,
    pub cache: TempDir,
    pub state: TempDir,
    pub out: TempDir,
}

pub fn fixture() -> Fixture {
    let cache = tempfile::tempdir().unwrap();
    let mut definitions = Vec::new();
    let mut normalized = Vec::new();
    for id in SOURCES {
        let n = normalized_fixture(id);
        record(cache.path(), id, &n.report);
        definitions.push(definition_of(id));
        normalized.push(n);
    }
    definitions.push(definition_of("researched-structures"));
    Fixture {
        definitions,
        normalized,
        structures: read_all_named(&workspace().join("structures")).unwrap(),
        cache,
        state: tempfile::tempdir().unwrap(),
        out: tempfile::tempdir().unwrap(),
    }
}

/// Records `report` as what normalization made of source `id` in the cache.
pub fn record(cache: &Path, id: &str, report: &NormalizationReport) {
    let dir = cache.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    Provenance::normalized(&dir, &source_id(id), report).unwrap();
}

/// The cross-match of the fixture against its state, its ids written as pending, as a run
/// leaves them.
pub fn cross(f: &Fixture) -> CrossMatched {
    let mut basis = Basis::read(&workspace().join("rules"), &[]).unwrap();
    basis.structures.clone_from(&f.structures);
    let release =
        Release::planck_2018().with_peculiar_velocity(basis.ranking.peculiar_velocity_km_s);
    let state = State::read(f.state.path()).unwrap();
    let inputs = MatchInputs {
        basis: &basis,
        release: &release,
        state: &state,
    };
    let entries = f
        .normalized
        .iter()
        .flat_map(|n| n.entries.clone())
        .collect();
    let crossed = cross_match(entries, &inputs, &RecordingSink::default()).unwrap();
    crossed.next.write_pending(f.state.path()).unwrap();
    crossed
}

pub fn tiles(f: &Fixture, crossed: &CrossMatched) -> Built {
    let rules = TileRules::read(&workspace().join("rules/tiles.yaml")).unwrap();
    let context = Context {
        release: &crossed.release,
        cosmology: &crossed.cosmology,
        rules: &rules,
        definitions: &f.definitions,
    };
    build_tiles(&crossed.objects, &context).unwrap()
}

pub const BUILT_AT: &str = "2026-11-03T12:00:00Z";

/// Builds `crossed` with `definitions`, and with `structures` in place of those the cross-match
/// worked under.
pub fn built(
    f: &Fixture,
    crossed: &CrossMatched,
    tiles: &Built,
    definitions: &[SourceDefinition],
    structures: &[(String, Structure)],
) -> Result<BuiltRelease, BuildError> {
    let mut crossed = crossed.clone();
    crossed.basis.structures = structures.to_vec();
    let inputs = Inputs {
        crossed: &crossed,
        tiles,
        definitions,
        grids: &[],
        cache: f.cache.path(),
        built_at: BUILT_AT.to_owned(),
    };
    build(&inputs, f.state.path(), f.out.path())
}

/// Cross-matches, lays out the tiles and builds the fixture's next release.
pub fn build_next(f: &Fixture) -> BuiltRelease {
    let crossed = cross(f);
    let tiles = tiles(f, &crossed);
    built(f, &crossed, &tiles, &f.definitions, &f.structures).unwrap()
}

pub fn read_manifest(dir: &Path) -> ReleaseManifest {
    let text = std::fs::read_to_string(dir.join(keys::MANIFEST)).unwrap();
    serde_norway::from_str(&text).unwrap()
}

/// The text of the release's D1 file of `table`.
pub fn d1_text(dir: &Path, manifest: &ReleaseManifest, table: &str) -> String {
    manifest
        .files
        .iter()
        .filter(|f| f.path.contains(&format!("-{table}-")))
        .map(|f| std::fs::read_to_string(dir.join(&f.path)).unwrap())
        .collect()
}
