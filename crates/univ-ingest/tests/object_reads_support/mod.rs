//! The fixture release loaded into `SQLite` and every cross-matched object decoded from its rows
//! as Univ Server reads them: each row a JSON map, as D1 returns it.

use std::sync::OnceLock;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Number, Value};
use univ_ingest::crossmatch::{CrossMatched, ReleaseObject};
use univ_model::api::objects::{MeasuredQuantity, MeasurementKey, ObjectRecord};
use univ_model::ids::Designation;
use univ_model::object::{
    CandidateLink, Confidence, LineOfSight, MatchMethod, MatchRecord, MeasurementRef, Object,
    Resolution,
};
use univ_model::release::read::{ObjectRows, Row, Vocabulary, object};

use crate::release_fixture::{built, cross, fixture_with, loaded, tiles};

/// Each cross-matched object beside the record its rows decode to.
pub struct Decoded {
    pub pairs: &'static [(ReleaseObject, ObjectRecord)],
}

struct Loaded {
    pairs: Vec<(ReleaseObject, ObjectRecord)>,
    /// Each object's `object_links` rows by their `n`, for the objects that have any.
    links: Vec<(u64, Vec<i64>)>,
}

/// The rows of `sql` with parameter `id`, as D1 gives them: integers and reals as JSON
/// numbers, text as strings, NULL as null.
pub fn rows(db: &Connection, sql: &str, id: i64) -> Vec<Row> {
    let mut q = db.prepare(sql).unwrap();
    let names: Vec<String> = q.column_names().map(str::to_owned).collect();
    q.query_map([id], |r| {
        let mut row = Row::new();
        for (i, name) in names.iter().enumerate() {
            let value = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => Value::Number(n.into()),
                ValueRef::Real(x) => Value::Number(Number::from_f64(x).unwrap()),
                ValueRef::Text(t) => Value::String(String::from_utf8(t.to_vec()).unwrap()),
                ValueRef::Blob(_) => unreachable!("a release holds no blob"),
            };
            row.insert(name.clone(), value);
        }
        Ok(row)
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// Release 1's vocabulary, as Univ Server reads it once per release.
fn vocabulary(db: &Connection) -> Vocabulary {
    let pairs = |sql: &str| -> Vec<(i64, String)> {
        let mut q = db.prepare(sql).unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let (cosmology, rule): (String, String) = db
        .query_row(
            "SELECT cosmology, ranking_rule_name FROM releases WHERE number = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let chains = pairs("SELECT id, steps FROM chains WHERE release = 1")
        .into_iter()
        .map(|(id, steps)| (id, serde_json::from_str(&steps).unwrap()));
    Vocabulary::new(
        1,
        serde_json::from_str(&cosmology).unwrap(),
        rule,
        pairs("SELECT id, term FROM terms WHERE release = 1"),
        chains,
        pairs("SELECT n, id FROM sources WHERE release = 1"),
    )
}

/// Candidate links between the first two objects, both ways, as cross-matching makes them: a
/// match by position and redshift whose velocities disagree, and a cross-identification that
/// two entries of one source would claim. The fixture's sources make none of their own; these
/// are made up to test how links are written and read, and no science rests on them.
fn with_links(crossed: &mut CrossMatched) {
    let a = crossed.objects[0].object.entries[0]
        .entry
        .designation
        .clone();
    let b = crossed.objects[1].object.entries[0]
        .entry
        .designation
        .clone();
    let confidence = Confidence::new(0.3).unwrap();
    let matched = |with: &Designation| {
        Resolution::Match(MatchRecord {
            method: MatchMethod::PositionAndRedshift,
            with: with.clone(),
            separation_arcsec: 12.5,
            difference: Some(LineOfSight::Velocity { delta_km_s: 450.0 }),
            confidence,
        })
    };
    let claimed = |via: &Designation| Resolution::CrossIdentification {
        scheme: "pgc".into(),
        identifier: "2557".into(),
        via: via.clone(),
        confidence: Confidence::NONE,
    };
    for (o, from, to) in [(0, &a, &b), (1, &b, &a)] {
        let links = &mut crossed.objects[o].object.links;
        links.push(CandidateLink {
            entry: from.clone(),
            candidate: to.clone(),
            reason: matched(from),
            confidence,
        });
        links.push(CandidateLink {
            entry: from.clone(),
            candidate: to.clone(),
            reason: claimed(from),
            confidence: Confidence::NONE,
        });
    }
}

fn load() -> Loaded {
    // DESI's redshifts are barycentric: its objects read back with that frame. An open
    // cluster, a Cepheid and a maser source add the Galaxy's tracers, in parsecs.
    let fixture = fixture_with(&[
        "desi-dr1",
        "hunt-reffert-2023",
        "skowron-2025-cepheids",
        "reid-2019-masers",
    ]);
    let mut crossed = cross(&fixture);
    with_links(&mut crossed);
    let tiles = tiles(&fixture, &crossed);
    let release = built(
        &fixture,
        &crossed,
        &tiles,
        &fixture.definitions,
        &fixture.structures,
    )
    .unwrap();
    let db = loaded(&release.dir, &release.manifest);
    let v = vocabulary(&db);
    let mut pairs = Vec::new();
    let mut links = Vec::new();
    for o in crossed.objects {
        let id = i64::try_from(o.id.get()).unwrap();
        let mut one = rows(
            &db,
            "SELECT * FROM objects WHERE release = 1 AND id = ?1",
            id,
        );
        assert_eq!(one.len(), 1, "{id}");
        let entries = rows(
            &db,
            "SELECT * FROM entries WHERE release = 1 AND object_id = ?1 ORDER BY entry DESC",
            id,
        );
        let measurements = rows(
            &db,
            "SELECT * FROM measurements WHERE release = 1 AND object_id = ?1 ORDER BY n DESC",
            id,
        );
        // As Univ Server reads them from the object's shard: each source by its number.
        let designations = rows(
            &db,
            "SELECT source, identifier FROM designations \
             WHERE release = 1 AND object_id = ?1 ORDER BY identifier DESC",
            id,
        );
        // As Univ Server reads them from the catalogue.
        let names = rows(
            &db,
            "SELECT source, name FROM names WHERE release = 1 AND object_id = ?1",
            id,
        );
        let of = |table: &str| {
            rows(
                &db,
                &format!("SELECT * FROM {table} WHERE release = 1 AND object_id = ?1"),
                id,
            )
        };
        let (details, flags, references) = (
            of("measurement_details"),
            of("entry_flags"),
            of("entry_references"),
        );
        let (object_links, relations) = (of("object_links"), of("object_relations"));
        if !object_links.is_empty() {
            let n = object_links
                .iter()
                .filter_map(|r| r["n"].as_i64())
                .collect();
            links.push((o.id.get(), n));
        }
        let record = object(
            &v,
            &ObjectRows {
                object: &one.remove(0),
                entries: &entries,
                measurements: &measurements,
                measurement_details: &details,
                entry_flags: &flags,
                entry_references: &references,
                links: &object_links,
                relations: &relations,
                designations: &designations,
                names: &names,
            },
        )
        .unwrap_or_else(|e| panic!("object {id}: {e}"));
        pairs.push((o, record));
    }
    Loaded { pairs, links }
}

static LOADED: OnceLock<Loaded> = OnceLock::new();

/// Every cross-matched object of the fixture beside its decoded record, built once.
pub fn decoded() -> Decoded {
    Decoded {
        pairs: &LOADED.get_or_init(load).pairs,
    }
}

/// Each object's `object_links` rows by their `n`, for every object that has any.
pub fn link_rows() -> Vec<(u64, Vec<i64>)> {
    LOADED.get_or_init(load).links.clone()
}

/// The key of measurement `m` of `object`: its entry's number, its quantity and n.
pub fn key_of(object: &Object, m: &MeasurementRef) -> MeasurementKey {
    let entry = object
        .entries
        .iter()
        .position(|e| &e.entry.designation == m.entry())
        .unwrap();
    let (quantity, n) = match m {
        MeasurementRef::Distance { index, .. } => (MeasuredQuantity::Distance, *index),
        MeasurementRef::Redshift { index, .. } => (MeasuredQuantity::Redshift, *index),
        MeasurementRef::Parallax { .. } => (MeasuredQuantity::Parallax, 0),
    };
    MeasurementKey {
        entry: u32::try_from(entry).unwrap(),
        quantity,
        n: u32::try_from(n).unwrap(),
    }
}
