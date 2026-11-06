//! The fixture release: a release directory laid out as a build lays it out, its D1 rows
//! written and fingerprinted with univ-model as a build does.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};
use univ_model::cosmology::Cosmology;
use univ_model::position::unit_vector;
use univ_model::release::fingerprint::canonical_row;
use univ_model::release::manifest::{
    CellIndex, Counts, D1Estimate, FORMAT, Ids, MANIFEST_VERSION, ManifestFile, TableEstimate,
    Target, Versions,
};
use univ_model::release::sql::write_tuple;
use univ_model::release::{Fingerprinter, ReleaseManifest, SCHEMA, SqlValue, TABLES, Table, keys};

use crate::science::fingerprints::hex;

/// The fixture's release number.
const N: i64 = 1;

/// The fixture's terms, numbered by their place.
const TERMS: [&str; 12] = [
    "nearby_galaxies",
    "cosmic_web",
    "body",
    "distance",
    "redshift",
    "megaparsec",
    "symmetric",
    "none_reported",
    "cepheids",
    "measured",
    "no_distance",
    "anchor",
];

fn term(name: &str) -> SqlValue {
    let i = TERMS.iter().position(|t| *t == name).unwrap();
    SqlValue::Integer(i64::try_from(i).unwrap())
}

fn table(name: &str) -> Table {
    *TABLES.iter().find(|t| t.name == name).unwrap()
}

/// The fixture's rows by table, each row from the columns given, every other column NULL.
#[derive(Default)]
struct Rows(BTreeMap<&'static str, Vec<Vec<SqlValue>>>);

impl Rows {
    fn add(&mut self, name: &'static str, values: &[(&str, SqlValue)]) {
        let t = table(name);
        let mut row = vec![SqlValue::Null; t.columns.len()];
        for (column, value) in values {
            row[t.position(column).unwrap()] = value.clone();
        }
        self.0.entry(name).or_default().push(row);
    }
}

fn text(t: &str) -> SqlValue {
    SqlValue::from(t)
}

fn int(n: i64) -> SqlValue {
    SqlValue::Integer(n)
}

fn real(x: f64) -> SqlValue {
    SqlValue::Real(x)
}

/// A galaxy object placed at `d` ± `sigma` Mpc toward (`ra`, `dec`) with a Cepheid distance of
/// source `source`, designated `identifier`.
fn galaxy(
    rows: &mut Rows,
    id: i64,
    (ra, dec, d, sigma): (f64, f64, f64, f64),
    source: i64,
    identifier: &str,
) {
    let [x, y, z] = unit_vector(ra, dec).map(|c| c * d);
    let domain = if d <= 11.0 {
        "nearby_galaxies"
    } else {
        "cosmic_web"
    };
    rows.add(
        "objects",
        &[
            ("release", int(N)),
            ("id", int(id)),
            ("kind", term("body")),
            ("ra", real(ra)),
            ("dec", real(dec)),
            ("placed", int(1)),
            ("scale_domain", term(domain)),
            ("x", real(x)),
            ("y", real(y)),
            ("z", real(z)),
            ("distance_mpc", real(d)),
            ("distance_form", term("symmetric")),
            ("distance_minus", real(sigma)),
            ("distance_plus", real(sigma)),
            ("preferred_entry", int(0)),
            ("preferred_quantity", term("distance")),
            ("preferred_n", int(0)),
        ],
    );
    rows.add(
        "entries",
        &[
            ("release", int(N)),
            ("object_id", int(id)),
            ("entry", int(0)),
            ("source", int(source)),
            ("identifier", text(identifier)),
            ("resolution", term("anchor")),
        ],
    );
    rows.add(
        "measurements",
        &[
            ("release", int(N)),
            ("object_id", int(id)),
            ("entry", int(0)),
            ("quantity", term("distance")),
            ("n", int(0)),
            ("kind_of_value", term("measured")),
            ("native_value", real(d)),
            ("native_unit", term("megaparsec")),
            ("native_form", term("symmetric")),
            ("native_minus", real(sigma)),
            ("native_plus", real(sigma)),
            ("method", term("cepheids")),
        ],
    );
    rows.add(
        "designations",
        &[
            ("release", int(N)),
            ("source", int(source)),
            ("identifier", text(identifier)),
            ("object_id", int(id)),
        ],
    );
}

fn fixture_rows() -> Rows {
    let mut rows = Rows::default();
    let empty = [
        "matching_rules",
        "ranking_rule",
        "ranking_rule_name",
        "sources",
        "model_version",
        "ingest_version",
    ];
    let mut release = vec![
        ("number", int(N)),
        ("built_at", text("2026-11-03T12:00:00Z")),
        ("cosmology", text("{}")),
        ("peculiar_velocity_km_s", real(300.0)),
        ("schema_version", int(2)),
        ("objects", int(4)),
        ("structures", int(0)),
        ("grids", int(1)),
        ("tiles", int(1)),
        ("d1_estimated_bytes", int(1000)),
    ];
    release.extend(empty.iter().map(|c| (*c, text("-"))));
    rows.add("releases", &release);
    for (i, t) in (0i64..).zip(TERMS) {
        rows.add(
            "terms",
            &[("release", int(N)), ("id", int(i)), ("term", text(t))],
        );
    }
    for (n, (id, domain)) in
        (0i64..).zip([("gal-a", "nearby_galaxies"), ("gal-b", "nearby_galaxies")])
    {
        let mut source = vec![
            ("release", int(N)),
            ("n", int(n)),
            ("id", text(id)),
            ("scale_domain", text(domain)),
            ("entries", int(2)),
        ];
        let empty = [
            "title",
            "provider",
            "citation",
            "terms_url",
            "terms_quote",
            "credit",
            "redistribution",
            "review",
            "access",
            "cuts",
            "fixes",
        ];
        source.extend(empty.iter().map(|c| (*c, text("-"))));
        rows.add("sources", &source);
    }
    // A galaxy, the same galaxy again from another source 0.38′ away, a distant galaxy, and an
    // object with no distance.
    galaxy(&mut rows, 1, (10.0, 41.0, 0.78, 0.02), 0, "A1");
    galaxy(&mut rows, 2, (10.005, 41.005, 0.79, 0.02), 1, "B1");
    galaxy(&mut rows, 3, (200.0, -30.0, 16.0, 1.0), 0, "A2");
    rows.add(
        "objects",
        &[
            ("release", int(N)),
            ("id", int(4)),
            ("kind", term("body")),
            ("placed", int(0)),
            ("not_placed", term("no_distance")),
        ],
    );
    rows.add(
        "names",
        &[
            ("release", int(N)),
            ("key", text("galaxya")),
            ("object_id", int(1)),
            ("source", int(0)),
            ("name", text("Galaxy A")),
        ],
    );
    rows
}

/// Writes `bytes` at `path` within `dir` and lists it as a file of the release.
fn file(dir: &Path, path: &str, bytes: &[u8], target: Target, files: &mut Vec<ManifestFile>) {
    let full = dir.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, bytes).unwrap();
    files.push(ManifestFile {
        path: path.to_owned(),
        target,
        key: (target == Target::R2).then(|| keys::file(&hex(&Sha256::digest(bytes)))),
        bytes: bytes.len() as u64,
        sha256: hex(&Sha256::digest(bytes)),
    });
}

/// Builds the fixture release in `dir`: its D1 files, a tile, a 2 × 2 × 2 label grid whose label
/// is 1 at negative x and 2 at positive x, and its manifest.
pub fn build(dir: &Path) {
    let mut rows = fixture_rows();
    let mut files = Vec::new();
    file(
        dir,
        "d1/0000-schema.sql",
        SCHEMA.as_bytes(),
        Target::D1,
        &mut files,
    );
    let mut sql = String::new();
    let mut tables = Vec::new();
    for t in &TABLES {
        let mut fp = Fingerprinter::new(*t);
        let mine = rows.0.entry(t.name).or_default();
        for row in mine.iter() {
            sql.push_str(&t.insert_head());
            write_tuple(row, &mut sql);
            sql.push_str(";\n");
            fp.add(row);
        }
        let content = t.checked_by_content().then(|| {
            t.sort_by_key(mine);
            let mut bytes = Vec::new();
            for row in mine.iter() {
                canonical_row(row, &mut bytes);
            }
            hex(&Sha256::digest(&bytes))
        });
        tables.push(TableEstimate {
            table: t.name.to_owned(),
            rows: mine.len() as u64,
            estimated_bytes: 100,
            fingerprint: fp.finish(),
            added: None,
            retired: None,
            content_sha256: content,
        });
    }
    file(
        dir,
        "d1/0001-rows.sql",
        sql.as_bytes(),
        Target::D1,
        &mut files,
    );
    file(dir, "tiles/stars/r0.bin", &[7; 100], Target::R2, &mut files);
    let samples: Vec<u8> = (0..8)
        .flat_map(|i: u8| f32::from(i % 2 + 1).to_le_bytes())
        .collect();
    file(dir, "grids/g/field-0.f32", &samples, Target::R2, &mut files);
    let header = serde_json::json!({
        "axes": ["z", "y", "x"], "shape": [2, 2, 2], "origin": [-1.0, -1.0, -1.0], "cell": 2.0,
        "unit": "megaparsec_over_h", "frame": "supergalactic",
        "fields": [{"samples": keys::file(&hex(&Sha256::digest(&samples))),
                    "labels": [{"label": 1, "name": "West"}, {"label": 2, "name": "East"}]}]
    });
    file(
        dir,
        "grids/g/header.json",
        header.to_string().as_bytes(),
        Target::R2,
        &mut files,
    );
    let yaml = serde_norway::to_string(&manifest(files, tables)).unwrap();
    std::fs::write(dir.join(keys::MANIFEST), yaml).unwrap();
}

/// The fixture's manifest of `files`, with the fingerprints of its `tables`.
fn manifest(files: Vec<ManifestFile>, tables: Vec<TableEstimate>) -> ReleaseManifest {
    ReleaseManifest {
        format: FORMAT.to_owned(),
        manifest_version: MANIFEST_VERSION,
        release: 1,
        built_at: "2026-11-03T12:00:00Z".to_owned(),
        cosmology: Cosmology::planck_2018(),
        cosmology_note: None,
        peculiar_velocity_km_s: 300.0,
        versions: Versions {
            univ_model: "0".into(),
            univ_ingest: "0".into(),
            d1_schema: 2,
            tile_encoding: 1,
            tile_manifest: 1,
        },
        rules: Vec::new(),
        cell_index: CellIndex {
            scheme: "-".into(),
            frame: "-".into(),
            spaces: Vec::new(),
            structure_span: 4,
            max_structure_level: 20,
            not_indexed: Vec::new(),
        },
        sources: Vec::new(),
        counts: Counts {
            objects: 4,
            placed: 3,
            entries: 3,
            structures: 0,
            structures_indexed: 0,
            grids: 1,
            tiles: 1,
            object_cells: 0,
            structure_cells: 0,
        },
        ids: Ids {
            issued_first: None,
            issued_last: None,
            replaced: 0,
        },
        parent: None,
        d1: D1Estimate {
            cap_bytes: 10_000_000_000,
            estimated_bytes: 1000,
            tables,
        },
        shards: Vec::new(),
        files,
    }
}
