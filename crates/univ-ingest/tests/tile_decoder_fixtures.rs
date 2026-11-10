//! The fixtures of the reference TypeScript tile decoder, `clients/tile-decoder/fixtures/`,
//! against what ingest builds now: each scale domain of the fixture release (real data) and one
//! SIMULATED domain of several levels, each as its tile manifest as written and its tiles with
//! each point's position before quantization, object id, source and kind of value. With
//! `UPDATE_FIXTURES=1` set, the test rewrites the files instead.
//!
//! It also reads each written manifest back through univ-model's `TileManifest`, unchanged.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;
mod simulated;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::{source_id, workspace};
use release_fixture::{cross, fixture, tiles};
use serde::Serialize;
use simulated::tiles::{Lcg, context, rules};
use univ_ingest::fetch::hex;
use univ_ingest::source::ScaleDomain;
use univ_ingest::tiles::{BuiltDomain, DomainPoint, assign, build_domain, name_of};
use univ_model::cosmology::Cosmology;
use univ_model::ids::ObjectId;
use univ_model::tile::manifest::{TileManifest, TileManifestIndex};
use univ_model::tile::{COLOUR_BANDS, ColourBands, KINDS, PointColour, decode};
use univ_model::value::KindOfValue;

/// The variable that asks the test to rewrite the files.
const UPDATE: &str = "UPDATE_FIXTURES";

/// The directory, relative to the workspace.
const DIR: &str = "clients/tile-decoder/fixtures";

/// How the files came about, after what each holds.
const ABOUT: &str = "as the Rust encoder writes them, with each point's position before \
                     quantization. Checked against what ingest builds by univ-ingest's test \
                     tile_decoder_fixtures; UPDATE_FIXTURES=1 cargo test -p univ-ingest --test \
                     tile_decoder_fixtures rewrites them.";

/// A domain's tiles file.
#[derive(Serialize)]
struct DomainFile {
    about: String,
    scale_domain: &'static str,
    tiles: Vec<Tile>,
}

/// A tile: its node's address, its bytes in hexadecimal, and its points in the order stored.
#[derive(Serialize)]
struct Tile {
    address: String,
    hex: String,
    points: Vec<Expected>,
}

/// What a point was built from.
#[derive(Serialize)]
struct Expected {
    id: u64,
    /// In the manifest's frame and unit, before quantization.
    position: [f64; 3],
    /// The id of the source of its preferred distance.
    source: String,
    kind: KindOfValue,
    /// Its colour index before rounding, and its bands; none where it has none.
    colour: Option<(f64, ColourBands)>,
}

/// A domain's built tiles, the points they were built from and the ids of their sources.
struct Domain {
    /// The name of its files.
    file: &'static str,
    /// What its tiles are, for the file's `about`.
    what: &'static str,
    built: BuiltDomain,
    points: Vec<DomainPoint>,
    sources: Vec<String>,
}

/// The fixture release's domains: real data.
fn release() -> Vec<Domain> {
    let f = fixture();
    let crossed = cross(&f);
    let built = tiles(&f, &crossed);
    let mut assigned = assign(&crossed.objects, &f.definitions, &crossed.cosmology).unwrap();
    built
        .domains
        .into_iter()
        .map(|built| {
            let p = assigned.domains.remove(&built.domain).unwrap_or_default();
            Domain {
                file: name_of(built.domain),
                what: "Tiles of Univ Ingest's fixture release, real data,",
                built,
                points: p.points,
                sources: p.sources.iter().map(ToString::to_string).collect(),
            }
        })
        .collect()
}

/// SIMULATED: 60 stars points at made-up positions under the ids of two real sources, every
/// kind of value among them, split over several levels.
fn simulated() -> Domain {
    let mut lcg = Lcg(11);
    let points: Vec<DomainPoint> = (1..=60u16)
        .map(|n| DomainPoint {
            id: ObjectId::new(u64::from(n) * 1_000_003).unwrap(),
            place: u32::from(n),
            position: [0, 1, 2].map(|_| (2.0 * lcg.next() - 1.0) * 300.0),
            source: n % 2,
            sky_source: n % 2,
            kind: KINDS[usize::from(n) % KINDS.len()],
            // Every third point has none; the rest alternate between the bands.
            colour: (n % 3 != 0).then(|| PointColour {
                index: f64::from(n) / 17.0 - 0.4,
                bands: COLOUR_BANDS[usize::from(n) % COLOUR_BANDS.len()],
            }),
        })
        .collect();
    let sources = [source_id("gaia-dr3-bright"), source_id("hipparcos-2007")];
    let (cosmology, definitions) = (Cosmology::planck_2018(), common::read_sources());
    let rules = rules(12, 8, 3, 5);
    let built = build_domain(
        ScaleDomain::Stars,
        &points,
        &sources,
        &context(&rules, &cosmology, &definitions),
    )
    .unwrap();
    Domain {
        file: "simulated",
        what: "SIMULATED tiles: made-up positions under the ids of real sources, built",
        built,
        points,
        sources: sources.iter().map(ToString::to_string).collect(),
    }
}

/// Every domain the fixtures hold.
fn domains() -> Vec<Domain> {
    let mut domains = release();
    domains.push(simulated());
    domains
}

/// The tiles file of `d`.
fn tiles_file(d: &Domain) -> String {
    let by_id: BTreeMap<u64, &DomainPoint> = d.points.iter().map(|p| (p.id.get(), p)).collect();
    let expected = |id: u64| {
        let built = by_id[&id];
        Expected {
            id,
            position: built.position,
            source: d.sources[usize::from(built.source)].clone(),
            kind: built.kind,
            colour: built.colour.map(|c| (c.index, c.bands)),
        }
    };
    let tiles = d
        .built
        .nodes
        .iter()
        .map(|n| Tile {
            address: n.address.clone(),
            hex: hex(&n.tile),
            points: decode(&n.tile)
                .unwrap()
                .points
                .iter()
                .map(|p| expected(p.id.get()))
                .collect(),
        })
        .collect();
    let tiles = DomainFile {
        about: format!("{} {ABOUT}", d.what),
        scale_domain: name_of(d.built.domain),
        tiles,
    };
    let mut text = serde_json::to_string_pretty(&tiles).unwrap();
    text.push('\n');
    text
}

/// Each file of the fixtures, by name, with its text.
fn files() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for d in DOMAINS.iter() {
        let manifest = String::from_utf8(d.built.json.clone()).unwrap();
        files.push((format!("{}.manifest.json", d.file), manifest));
        files.push((format!("{}.tiles.json", d.file), tiles_file(d)));
    }
    files
}

fn dir() -> PathBuf {
    workspace().join(DIR)
}

#[test]
fn the_tile_decoders_fixtures_are_what_ingest_builds() {
    let files = files();
    if std::env::var_os(UPDATE).is_some() {
        std::fs::create_dir_all(dir()).unwrap();
        for (name, text) in &files {
            std::fs::write(dir().join(name), text).unwrap();
        }
        return;
    }
    for (name, text) in &files {
        let file = std::fs::read_to_string(dir().join(name)).unwrap_or_default();
        assert!(
            file == *text,
            "{DIR}/{name} differs from what ingest builds: run {UPDATE}=1 cargo test -p \
             univ-ingest --test tile_decoder_fixtures to rewrite it"
        );
    }
    let mut present: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| {
            Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    present.sort();
    let mut expected: Vec<String> = files.into_iter().map(|(n, _)| n).collect();
    expected.sort();
    assert_eq!(present, expected, "{DIR} holds no other fixture");
}

#[test]
fn each_written_manifest_reads_back_through_univ_models_type_unchanged() {
    for d in DOMAINS.iter() {
        let read: TileManifest = serde_json::from_slice(&d.built.json).unwrap();
        assert_eq!(read, d.built.manifest);
        assert_eq!(read.to_json().unwrap(), d.built.json, "{}", d.file);
        let index: TileManifestIndex = serde_json::from_slice(&d.built.json).unwrap();
        assert_eq!(index.encoding.version, read.encoding.version);
        let nodes: Vec<(&str, &str)> = index
            .nodes
            .iter()
            .map(|n| (n.address.as_str(), n.sha256.as_str()))
            .collect();
        let expected: Vec<(&str, &str)> = read
            .nodes
            .iter()
            .map(|n| (n.address.as_str(), n.sha256.as_str()))
            .collect();
        assert_eq!(nodes, expected, "{}", d.file);
    }
}

/// Every domain the fixtures hold, built once for both tests.
static DOMAINS: std::sync::LazyLock<Vec<Domain>> = std::sync::LazyLock::new(domains);
