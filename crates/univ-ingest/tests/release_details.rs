//! The fixture release's `source_details` and `structure_details` rows (schema version 3),
//! loaded into `SQLite`: each source's primary reference and coverage, and each structure's
//! placed shape and derivation, outline test and rescaled lengths.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use release_fixture::{build_next, fixture};
use rusqlite::Connection;
use serde_json::Value;
use univ_model::api::sources::{RadialRange, SkyFootprint};
use univ_model::api::structures::{Derivation, OutlineRecord, RescaledLength};
use univ_model::geometry::Shape;
use univ_model::release::manifest::Target;

/// The release's D1 files executed in path order into a fresh database.
fn loaded(dir: &std::path::Path, m: &univ_model::release::ReleaseManifest) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    for f in m.files.iter().filter(|f| f.target == Target::D1) {
        let sql = std::fs::read_to_string(dir.join(&f.path)).unwrap();
        db.execute_batch(&sql).unwrap();
    }
    db
}

fn json(text: Option<String>) -> Option<Value> {
    text.map(|t| serde_json::from_str(&t).unwrap())
}

/// A JSON column read as univ-model's typed contract for it, which it must be.
fn typed<T: serde::de::DeserializeOwned>(column: Option<&Value>) -> Option<T> {
    column.map(|v| serde_json::from_value(v.clone()).unwrap())
}

/// A source's id, bibcode, DOI, reference note, sky and radial columns.
type SourceDetail = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
    Option<Value>,
);

#[test]
fn the_fixture_release_records_each_sources_reference_and_coverage() {
    let f = fixture();
    let built = build_next(&f);
    let db = loaded(&built.dir, &built.manifest);
    let mut q = db
        .prepare(
            "SELECT s.id, d.bibcode, d.doi, d.reference_note, d.sky, d.radial \
             FROM source_details d JOIN sources s ON s.release = d.release AND s.n = d.source \
             WHERE d.release = 1 ORDER BY d.source",
        )
        .unwrap();
    let rows: Vec<SourceDetail> = q
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                json(r.get(4)?),
                json(r.get(5)?),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), f.definitions.len());
    for (id, bibcode, doi, note, sky, radial) in &rows {
        let d = f.definitions.iter().find(|d| d.id.as_str() == id).unwrap();
        assert_eq!(bibcode, &d.reference.bibcode, "{id}");
        assert_eq!(doi, &d.reference.doi, "{id}");
        assert_eq!(note, &d.reference.none, "{id}");
        if id == "researched-structures" {
            // A compilation cited per structure, holding no catalogue entry.
            assert!(note.is_some() && sky.is_none() && radial.is_none());
            continue;
        }
        let footprint: SkyFootprint = typed(sky.as_ref()).unwrap();
        assert_eq!(footprint.order, 5, "{id}");
        let ranges: Vec<RadialRange> = typed(radial.as_ref()).unwrap();
        assert!(!ranges.is_empty(), "{id}");
        let sky = sky.as_ref().unwrap();
        assert_eq!(sky["order"], 5, "{id}");
        let cells = sky["cells"].as_array().unwrap();
        assert!(!cells.is_empty() && cells.len() <= 5, "{id}: {sky}");
        assert!(radial.is_some(), "{id}");
    }
    let twomrs = rows.iter().find(|r| r.0 == "twomrs").unwrap();
    assert_eq!(twomrs.1.as_deref(), Some("2012ApJS..199...26H"));
    assert_eq!(twomrs.2.as_deref(), Some("10.1088/0067-0049/199/2/26"));
    assert_eq!(twomrs.5.as_ref().unwrap()[0]["measure"], "redshift");
}

#[test]
fn the_fixture_release_records_each_structures_shape_outline_and_rescaled_lengths() {
    let f = fixture();
    let built = build_next(&f);
    let db = loaded(&built.dir, &built.manifest);
    let mut q = db
        .prepare(
            "SELECT s.id, s.record, s.centre_x, d.shape, d.derivation, d.outline, d.rescaled \
             FROM structure_details d JOIN structures s ON s.release = d.release AND s.id = d.id \
             WHERE d.release = 1",
        )
        .unwrap();
    let mut rows = 0;
    let mut placed = 0;
    let mut rows_iter = q.query([]).unwrap();
    while let Some(r) = rows_iter.next().unwrap() {
        rows += 1;
        let id: String = r.get(0).unwrap();
        let record: Value = serde_json::from_str(&r.get::<_, String>(1).unwrap()).unwrap();
        let centre: Option<f64> = r.get(2).unwrap();
        let shape: Option<String> = r.get(3).unwrap();
        let derivation = json(r.get(4).unwrap());
        let outline = json(r.get(5).unwrap());
        let rescaled = json(r.get(6).unwrap());
        let _: Option<Derivation> = typed(derivation.as_ref());
        let _: Option<OutlineRecord> = typed(outline.as_ref());
        let _: Option<Vec<RescaledLength>> = typed(rescaled.as_ref());
        // A structure with a centre has a shape, which univ-model reads back and checks.
        assert_eq!(centre.is_some(), shape.is_some(), "{id}");
        if let Some(shape) = shape {
            placed += 1;
            let shape: Shape = serde_json::from_str(&shape).unwrap();
            assert_eq!(shape.frame, univ_model::position::Frame::Icrs, "{id}");
            let d = derivation.unwrap();
            for p in d["from"].as_array().unwrap() {
                assert!(record.pointer(p.as_str().unwrap()).is_some(), "{id}: {p}");
            }
        } else {
            assert!(derivation.is_none(), "{id}");
        }
        // The Local Void is outlined on CF4++, which the fixture release does not hold.
        assert_eq!(outline.is_some(), id == "local-void", "{id}");
        if let Some(o) = outline {
            assert_eq!(o["verdict"], "refused");
            assert_eq!(o["grid"], "cf4pp-density");
            assert!(
                o["reason"]
                    .as_str()
                    .unwrap()
                    .contains("not among this release's grids")
            );
        }
        if id == "bootes-void" {
            let r = rescaled.unwrap();
            assert!(
                r.as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i["method"] == "hubble_ratio")
            );
        }
    }
    assert_eq!(rows, f.structures.len());
    assert!(placed >= 20, "{placed}");
}
