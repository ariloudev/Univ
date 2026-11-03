//! The fixture release loaded into `SQLite` and every cross-matched object decoded from its rows
//! as Univ Server reads them: each row a JSON map, as D1 returns it.

use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde::Deserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Number, Value};
use univ_ingest::crossmatch::{CrossMatched, ReleaseObject};
use univ_model::api::objects::{MeasuredQuantity, MeasurementKey, ObjectRecord};
use univ_model::ids::Designation;
use univ_model::object::{
    CandidateLink, Confidence, LineOfSight, MatchMethod, MatchRecord, MeasurementRef, Object,
    Resolution,
};
use univ_model::release::manifest::Target;
use univ_model::release::read::{Row, Vocabulary, object};

use crate::release_fixture::{built, cross, fixture, tiles};

/// Each cross-matched object beside the record its rows decode to.
pub struct Decoded {
    pub pairs: &'static [(ReleaseObject, ObjectRecord)],
}

struct Loaded {
    pairs: Vec<(ReleaseObject, ObjectRecord)>,
    links: Vec<String>,
}

/// The release's D1 files executed in path order into a fresh database.
fn loaded(dir: &std::path::Path, m: &univ_model::release::ReleaseManifest) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    for f in m.files.iter().filter(|f| f.target == Target::D1) {
        let sql = std::fs::read_to_string(dir.join(&f.path)).unwrap();
        db.execute_batch(&sql).unwrap();
    }
    db
}

/// The rows of `sql` with parameter `id`, as D1 gives them: integers and reals as JSON
/// numbers, text as strings, NULL as null.
pub fn rows(db: &Connection, sql: &str, id: i64) -> Vec<Row> {
    let mut q = db.prepare(sql).unwrap();
    let names: Vec<String> = q.column_names().iter().map(|s| (*s).to_owned()).collect();
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
    let fixture = fixture();
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
        let record = object(&v, &one.remove(0), &entries, &measurements)
            .unwrap_or_else(|e| panic!("object {id}: {e}"));
        pairs.push((o, record));
    }
    let mut q = db
        .prepare("SELECT links FROM objects WHERE release = 1 AND links IS NOT NULL")
        .unwrap();
    let links = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Loaded { pairs, links }
}

static LOADED: OnceLock<Loaded> = OnceLock::new();

/// Every cross-matched object of the fixture beside its decoded record, built once.
pub fn decoded() -> Decoded {
    Decoded {
        pairs: &LOADED.get_or_init(load).pairs,
    }
}

/// The text of every `objects.links` column of the release that is not NULL.
pub fn link_texts() -> Vec<String> {
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

/// Any JSON value, refused when an object of it holds a key twice.
struct Unique;

impl<'de> DeserializeSeed<'de> for Unique {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Unique {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(Unique)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom(format!("the key {key:?} appears twice")));
            }
            map.next_value_seed(Unique)?;
        }
        Ok(())
    }
}

/// Whether `text` is JSON whose every object holds each key once.
pub fn no_duplicate_keys(text: &str) -> Result<(), serde_json::Error> {
    let mut d = serde_json::Deserializer::from_str(text);
    Unique.deserialize(&mut d)?;
    d.end()
}
