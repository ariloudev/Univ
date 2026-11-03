//! What Univ Server reads of a release, read back from its compact D1 rows (schema version 3)
//! loaded into `SQLite`: every derived position whole, its transformation chain from the
//! release's chain template and the object's parameters, every measurement's native value
//! beside its normalized one, and every designation and common name resolving to its object.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::collections::HashMap;

use release_fixture::{build_next, cross, fixture};
use rusqlite::{Connection, OptionalExtension, Row};
use serde_json::{Value, json};
use univ_model::object::MeasurementRef;
use univ_model::release::chains::expand;
use univ_model::release::manifest::Target;
use univ_model::release::rows::name_key;
use univ_model::release::{UncertaintyColumns, keys};

/// The release's D1 files executed in path order into a fresh database.
fn loaded(dir: &std::path::Path, m: &univ_model::release::ReleaseManifest) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    for f in m.files.iter().filter(|f| f.target == Target::D1) {
        let sql = std::fs::read_to_string(dir.join(&f.path)).unwrap();
        db.execute_batch(&sql).unwrap();
    }
    db
}

/// Release 1's terms by id.
fn terms(db: &Connection) -> HashMap<i64, String> {
    let mut q = db
        .prepare("SELECT id, term FROM terms WHERE release = 1")
        .unwrap();
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The quantity in columns `value`, `unit` and `{prefix}form`, `minus`, `plus`, `level` of a
/// row, as univ-model serializes it.
fn quantity(r: &Row<'_>, value: &str, unit: &str, prefix: &str, t: &HashMap<i64, String>) -> Value {
    let term = |c: &str| t[&r.get::<_, i64>(c).unwrap()].clone();
    let form = term(&format!("{prefix}form"));
    let columns = UncertaintyColumns {
        form: Box::leak(form.into_boxed_str()),
        minus: r.get(format!("{prefix}minus").as_str()).unwrap(),
        plus: r.get(format!("{prefix}plus").as_str()).unwrap(),
        level: r.get(format!("{prefix}level").as_str()).unwrap(),
    };
    json!({
        "value": r.get::<_, f64>(value).unwrap(),
        "unit": term(unit),
        "uncertainty": serde_json::to_value(columns.uncertainty().unwrap()).unwrap(),
    })
}

/// The fixture cross-matched, and its release built and loaded with its terms.
struct Loaded {
    crossed: univ_ingest::crossmatch::CrossMatched,
    db: Connection,
    terms: HashMap<i64, String>,
}

fn load() -> Loaded {
    let fixture = fixture();
    let crossed = cross(&fixture);
    let release = build_next(&fixture);
    assert!(release.dir.join(keys::MANIFEST).exists());
    let db = loaded(&release.dir, &release.manifest);
    let terms = terms(&db);
    Loaded { crossed, db, terms }
}

/// The preferred measurement row of object `id` and its chain: (chain, parameters, entry,
/// quantity term, n).
type Preferred = (i64, [Option<f64>; 4], i64, i64, i64);

fn preferred(db: &Connection, id: i64) -> Preferred {
    db.query_row(
        "SELECT chain, chain_p1, chain_p2, chain_p3, chain_p4, preferred_entry, \
         preferred_quantity, preferred_n FROM objects WHERE release = 1 AND id = ?1",
        [id],
        |r| {
            Ok((
                r.get(0)?,
                [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?],
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        },
    )
    .unwrap()
}

#[test]
fn every_derived_position_reads_back_with_its_chain_from_the_compact_rows() {
    let Loaded { crossed, db, terms } = load();
    let mut placed = 0;
    for o in &crossed.objects {
        let Some(position) = &o.position else {
            continue;
        };
        placed += 1;
        let id = i64::try_from(o.id.get()).unwrap();
        let (chain, params, entry, quantity_term, number) = preferred(&db, id);
        // The position's native value is its preferred measurement's (a redshift's CMB value).
        let native = db
            .query_row(
                "SELECT * FROM measurements WHERE release = 1 AND object_id = ?1 AND entry = ?2 \
                 AND quantity = ?3 AND n = ?4",
                [id, entry, quantity_term, number],
                |r| {
                    Ok(if terms[&quantity_term] == "redshift" {
                        let mut cmb = quantity(r, "cmb", "native_unit", "cmb_", &terms);
                        cmb["unit"] = json!("dimensionless");
                        cmb
                    } else {
                        quantity(r, "native_value", "native_unit", "native_", &terms)
                    })
                },
            )
            .unwrap();
        assert_eq!(
            native,
            serde_json::to_value(position.native()).unwrap(),
            "{id}"
        );
        let steps: String = db
            .query_row(
                "SELECT steps FROM chains WHERE release = 1 AND id = ?1",
                [chain],
                |r| r.get(0),
            )
            .unwrap();
        let template: Value = serde_json::from_str(&steps).unwrap();
        assert_eq!(
            expand(&template, &params, &native),
            Some(serde_json::to_value(position.chain()).unwrap()),
            "object {id}"
        );
        let expected = match &o.object.preferred_distance.as_ref().unwrap().measurement {
            MeasurementRef::Distance { .. } => "distance",
            MeasurementRef::Redshift { .. } => "redshift",
            MeasurementRef::Parallax { .. } => "parallax",
        };
        assert_eq!(terms[&quantity_term], expected);
    }
    assert!(placed > 10, "{placed}");
}

#[test]
fn every_distance_keeps_its_native_value_beside_its_normalized_one() {
    let Loaded { crossed, db, terms } = load();
    let distance = terms.iter().find(|(_, v)| *v == "distance").unwrap().0;
    let mut read = 0;
    for o in &crossed.objects {
        let id = i64::try_from(o.id.get()).unwrap();
        for (e, entry) in (0i64..).zip(&o.object.entries) {
            for (n, d) in (0i64..).zip(&entry.entry.distances) {
                let (native, value): (Value, Option<f64>) = db
                    .query_row(
                        "SELECT * FROM measurements WHERE release = 1 AND object_id = ?1 AND \
                         entry = ?2 AND quantity = ?3 AND n = ?4",
                        [id, e, *distance, n],
                        |r| {
                            let native =
                                quantity(r, "native_value", "native_unit", "native_", &terms);
                            Ok((native, r.get("value")?))
                        },
                    )
                    .unwrap();
                assert_eq!(native, serde_json::to_value(d.native).unwrap());
                assert_eq!(value.unwrap_or(d.native.value), d.normalized.value);
                read += 1;
            }
        }
    }
    assert!(read > 0);
}

#[test]
fn every_designation_and_name_resolves_to_its_object() {
    let Loaded { crossed, db, .. } = load();
    for o in &crossed.objects {
        let id = i64::try_from(o.id.get()).unwrap();
        for d in &o.object.designations {
            let found: Option<i64> = db
                .query_row(
                    "SELECT d.object_id FROM designations d JOIN sources s ON s.release = \
                     d.release AND s.n = d.source WHERE d.release = 1 AND s.id = ?1 AND \
                     d.identifier = ?2",
                    [d.source.as_str(), d.identifier.as_str()],
                    |r| r.get(0),
                )
                .optional()
                .unwrap();
            assert_eq!(found, Some(id), "{d:?}");
        }
        for name in &o.object.names {
            let found: i64 = db
                .query_row(
                    "SELECT object_id FROM names WHERE release = 1 AND key = ?1",
                    [name_key(&name.name)],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found, id, "{}", name.name);
        }
    }
}

#[test]
fn every_placed_object_s_scale_domain_follows_its_placed_distance() {
    // AC-92: stars where the preferred distance's source is a star source; otherwise nearby
    // galaxies within 11 Mpc and cosmic web beyond, whatever the source.
    let fixture = fixture();
    let Loaded { crossed, db, terms } = load();
    let domain_of_source = |s: &univ_model::ids::SourceId| {
        fixture
            .definitions
            .iter()
            .find(|d| &d.id == s)
            .unwrap()
            .scale_domain
    };
    let mut seen = std::collections::BTreeSet::new();
    for o in &crossed.objects {
        let id = i64::try_from(o.id.get()).unwrap();
        let term: Option<i64> = db
            .query_row(
                "SELECT scale_domain FROM objects WHERE release = 1 AND id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        let Some(position) = &o.position else {
            assert_eq!(term, None, "{id}");
            continue;
        };
        let source = &o.object.preferred_distance.as_ref().unwrap().measurement;
        let expected = univ_ingest::tiles::domain_of(
            domain_of_source(&source.entry().source),
            position.distance().value,
        );
        let name = &terms[&term.unwrap()];
        assert_eq!(name, univ_ingest::tiles::name_of(expected), "{id}");
        seen.insert(name.clone());
    }
    assert!(seen.len() >= 2, "{seen:?}");
}
