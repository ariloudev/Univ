//! `univ-ingest tiles --offline` run as the owner runs it, on the recorded answers of five
//! `VizieR` sources: the tiles stage's lines, and every tile it writes decoded by the Rust
//! reference decoder against its manifest.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use common::{copied, workspace};

use std::collections::BTreeSet;
use std::path::Path;

use univ_ingest::acquire::acquire;
use univ_ingest::source::Access;
use univ_ingest::tiles::{DOMAINS, MANIFEST_FILE, name_of, sha256, tile_file};
use univ_model::tile::{MAX_TILE_BYTES, decode};
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Sources each fetched in one query, whose recorded answers the test serves: Hipparcos stars,
/// nearby galaxies and cosmic web galaxies.
const SOURCES: [&str; 5] = [
    "hipparcos-2007",
    "mcconnachie-2012",
    "updated-nearby-galaxies",
    "twomrs",
    "cosmicflows-4",
];

fn tiling(definitions: &Path, cache: &Path, state: &Path, out: &Path) -> std::process::Output {
    common::run(
        "tiles",
        &[
            ("--definitions", definitions),
            ("--cache", cache),
            ("--state", state),
            ("--out", out),
        ],
    )
}

/// The directory name of each scale domain, in the order of their manifests.
fn domains() -> [&'static str; 3] {
    DOMAINS.map(name_of)
}

/// Every file each domain's directory under `out` holds, by path, with its bytes.
fn files(out: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for domain in domains() {
        let mut names: Vec<_> = std::fs::read_dir(out.join(domain))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        for name in names {
            let bytes = std::fs::read(out.join(domain).join(&name)).unwrap();
            files.push((format!("{domain}/{name}"), bytes));
        }
    }
    files
}

/// Decodes every tile of the domain under `out` against its manifest, giving its points.
fn check_domain(out: &Path, domain: &str) -> usize {
    let dir = out.join(domain);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(manifest["scale_domain"], domain);
    let sources = manifest["sources"].as_array().unwrap().len();
    let mut ids = BTreeSet::new();
    for node in manifest["nodes"].as_array().unwrap() {
        let address = node["address"].as_str().unwrap();
        let bytes = std::fs::read(dir.join(tile_file(address))).unwrap();
        assert!(bytes.len() <= MAX_TILE_BYTES);
        assert_eq!(node["bytes"], bytes.len());
        assert_eq!(node["sha256"], sha256(&bytes).as_str());
        let tile = decode(&bytes).unwrap();
        assert_eq!(node["points"], tile.points.len());
        let (min, max) = (tile.node.min(), tile.node.max());
        assert_eq!(node["bounds"]["min"], serde_json::json!(min));
        for p in &tile.points {
            assert!(ids.insert(p.id), "{} repeated", p.id);
            assert!(usize::from(p.source) < sources);
            assert!((0..3).all(|i| p.position[i] >= min[i] && p.position[i] <= max[i]));
        }
    }
    assert_eq!(manifest["points"], ids.len());
    ids.len()
}

#[tokio::test]
async fn an_offline_tiles_run_writes_tiles_the_decoder_reads_back_and_prints_its_counts() {
    let server = MockServer::start().await;
    let definitions = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    for id in SOURCES {
        let definition = common::definition_of(id);
        let Access::VizierTsv { table, .. } = &definition.access else {
            unreachable!()
        };
        let body = std::fs::read(common::fixture_of(&definition)).unwrap();
        Mock::given(query_param("-source", table.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
    }
    for id in SOURCES {
        let definition = copied(id, definitions.path(), &server.uri());
        let fetcher = &common::rig(false).fetcher;
        acquire(fetcher, &definition, cache.path(), &workspace())
            .await
            .unwrap();
    }

    let (state, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let run = tiling(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(run.status.success(), "{log}");
    for line in [
        // M81's 2MRS redshift alone is about 600% uncertain: it is not placed, and in no tile.
        "Z Step cross-match: positions: 16 placed of 25\n",
        "Z Stage tiles: 3 items\n",
        "Z Step tiles: stars: 1 nodes, 1 points, largest tile 65 bytes\n",
        "Z Step tiles: nearby galaxies: 1 nodes, 8 points, largest tile 184 bytes\n",
        "Z Step tiles: cosmic web: 1 nodes, 7 points, largest tile 167 bytes\n",
        "Z tiles: 1 stars nodes, 1 stars points, 65 stars largest tile bytes, 1 nearby galaxies \
         nodes, 8 nearby galaxies points, 184 nearby galaxies largest tile bytes, 1 cosmic web \
         nodes, 7 cosmic web points, 167 cosmic web largest tile bytes, 9 unplaced objects in no \
         tile\n",
        "Z Stopped at tiles: tiles only; no release was asked for\n",
    ] {
        assert!(log.contains(line), "{line}in\n{log}");
    }
    let points: Vec<usize> = domains()
        .into_iter()
        .map(|d| check_domain(out.path(), d))
        .collect();
    assert_eq!(points, [1, 8, 7], "the 16 placed objects, each in one tile");

    // The same run again, from a fresh state, writes the same bytes.
    let (again, out_again) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let rerun = tiling(
        definitions.path(),
        cache.path(),
        again.path(),
        out_again.path(),
    );
    assert!(rerun.status.success());
    assert_eq!(files(out.path()), files(out_again.path()));
}
