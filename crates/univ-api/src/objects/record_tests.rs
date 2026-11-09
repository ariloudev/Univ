//! Objects read through their routes (look-up-an-object), from release 2 of Univ Ingest's
//! fixture: the record of an object, and the answers that lead to one.

use serde_json::Value;

use crate::http::Response;
use crate::testkit::{FIXTURE_OBJECTS, RELEASES, Sqlite};

fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(FIXTURE_OBJECTS);
    db
}

fn read(db: &Sqlite, path: &str, query: &str) -> (Response, Value) {
    db.get_json(path, query)
}

fn object(id: u64) -> Value {
    let (response, record) = read(&db(), &format!("/v1/objects/{id}"), "");
    assert_eq!(response.status, 200, "{record}");
    assert_eq!(response.header("univ-release"), Some("2"));
    record
}

#[test]
fn every_entry_is_listed_with_its_source_and_designation() {
    let record = object(11);
    let entry = &record["entries"][0];
    assert_eq!(entry["source"], "mcconnachie-2012");
    assert_eq!(entry["identifier"], "Canis Major");
    let designations = record["designations"].as_array().unwrap();
    assert!(
        designations.contains(
            &serde_json::json!({"source": "mcconnachie-2012", "identifier": "Canis Major"})
        )
    );
}

#[test]
fn every_measurement_has_its_native_value_unit_uncertainty_kind_and_source() {
    let record = object(11);
    let measurements = record["measurements"].as_array().unwrap();
    assert_ne!(measurements.len(), 0);
    for m in measurements {
        assert!(m["native"]["value"].is_number(), "{m}");
        assert!(m["native"]["unit"].is_string(), "{m}");
        assert!(m["native"]["uncertainty"]["form"].is_string(), "{m}");
        assert!(m["kind_of_value"].is_string(), "{m}");
        assert_eq!(m["source"], "mcconnachie-2012");
    }
}

#[test]
fn a_redshift_whose_source_states_one_uncertainty_for_its_catalogue_is_estimated() {
    let db = db();
    let redshift = |record: &Value| -> Value {
        record["measurements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["quantity"] == "redshift")
            .cloned()
            .unwrap()
    };
    let (_, before) = read(&db, "/v1/objects/1", "");
    let published = redshift(&before);
    assert!(published.get("uncertainty_basis").is_none(), "{published}");
    let source = published["source"].as_str().unwrap().to_owned();
    // A fresh backend: the vocabulary of a release is read once and kept.
    let db = self::db();
    db.exec(&format!(
        "INSERT INTO source_terms SELECT 2, n, NULL, NULL, \
         '[{{\"quantity\":\"redshift\",\"value\":85.0,\"unit\":\"kilometre_per_second\",\
         \"times_one_plus_z\":false,\"basis\":\"Colless et al. 2001: rms 85 km/s\"}}]', NULL \
         FROM sources WHERE release = 2 AND id = '{source}';"
    ));
    let (_, after) = read(&db, "/v1/objects/1", "");
    assert_eq!(
        redshift(&after)["uncertainty_basis"],
        serde_json::json!({"kind_of_value": "estimated", "basis": "Colless et al. 2001: rms 85 km/s"})
    );
    // Only the quantity it is stated for.
    for m in after["measurements"].as_array().unwrap() {
        if m["quantity"] != "redshift" {
            assert!(m.get("uncertainty_basis").is_none(), "{m}");
        }
    }
}

#[test]
fn the_preferred_measurement_names_the_ranking_rule_that_chose_it() {
    let preferred = &object(11)["preferred"];
    assert_eq!(
        preferred["rule"],
        "measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method"
    );
    assert_eq!(preferred["measurement"]["entry"], 0);
    assert!(preferred["decided"].is_string(), "{preferred}");
}

#[test]
fn a_derived_position_names_its_frame_epoch_cosmology_and_transformation() {
    let position = &object(11)["position"];
    assert_eq!(position["frame"], "ICRS equatorial Cartesian");
    assert_eq!(position["origin"], "Solar System barycentre");
    assert_eq!(position["cosmology"]["hubble_constant"], 67.66);
    assert!(position["epoch"].is_number(), "{position}");
    assert_ne!(position["chain"].as_array().unwrap().len(), 0);
}

#[test]
fn an_object_with_no_distance_has_a_sky_position_and_no_derived_position() {
    // HIP 3, whose Hipparcos-2 parallax is too uncertain to invert.
    let record = object(8);
    assert_eq!(record["position"], Value::Null);
    assert!(record["sky_position"]["ra"].is_number(), "{record}");
    assert!(record["not_placed"].is_string(), "{record}");
}

#[test]
fn a_common_name_resolves_ignoring_letter_case_and_white_space() {
    for name in ["Canis Major", "canismajor", "  CANIS   major"] {
        let (response, record) = read(&db(), "/v1/objects", &format!("name={name}"));
        assert_eq!(response.status, 200, "{name}: {record}");
        assert_eq!(record["id"], 11, "{name}");
    }
}

#[test]
fn a_designation_resolves_to_its_object() {
    let (response, record) = read(
        &db(),
        "/v1/objects",
        "source=mcconnachie-2012&designation=Sagittarius+dSph",
    );
    assert_eq!(response.status, 200, "{record}");
    assert_eq!(record["id"], 12);
}

#[test]
fn a_lookup_that_finds_nothing_answers_404_naming_what_was_looked_up() {
    let db = db();
    let (response, problem) = read(&db, "/v1/objects", "name=Nowhere");
    assert_eq!(response.status, 404);
    assert_eq!(problem["type"], "urn:univ:problem:object-not-found");
    assert_eq!(problem["looked_up"]["name"], "Nowhere");
    let (response, problem) = read(&db, "/v1/objects/99999", "");
    assert_eq!(response.status, 404);
    assert_eq!(problem["looked_up"]["id"], 99_999);
    let (response, problem) = read(
        &db,
        "/v1/objects",
        "source=hipparcos-2007&designation=nothing",
    );
    assert_eq!(response.status, 404);
    assert_eq!(problem["looked_up"]["designation"], "nothing");
}

#[test]
fn a_lookup_with_neither_a_name_nor_a_designation_is_refused() {
    for query in [
        "",
        "source=hipparcos-2007",
        "designation=1",
        "name=M31&source=x",
    ] {
        let (response, _) = read(&db(), "/v1/objects", query);
        assert_eq!(response.status, 400, "{query}");
    }
    let (response, _) = read(&db(), "/v1/objects/abc", "");
    assert_eq!(response.status, 400);
}

#[test]
fn the_release_vocabulary_is_read_once() {
    let db = db();
    let before = db.statements.get();
    object_from(&db, 11);
    let first = db.statements.get() - before;
    object_from(&db, 12);
    let second = db.statements.get() - before - first;
    assert_eq!(first - second, 1, "the second read keeps the vocabulary");
}

fn object_from(db: &Sqlite, id: u64) {
    let (response, _) = read(db, &format!("/v1/objects/{id}"), "");
    assert_eq!(response.status, 200);
}

#[test]
fn designations_are_given_raw_and_in_their_catalogues_form_highest_ranked_first() {
    let record = object(6);
    let first = &record["designations"][0];
    assert_eq!(first["source"], "hipparcos-2007");
    assert_eq!(first["identifier"], "1");
    assert_eq!(first["designation"], "HIP 1");
    assert_eq!(record["entries"][0]["designation"], "HIP 1");
    // A source whose definition gives no form gives its identifier alone.
    let canis = object(11);
    assert_eq!(
        canis["designations"][0],
        serde_json::json!({"source": "mcconnachie-2012", "identifier": "Canis Major"})
    );
}

#[test]
fn each_entry_keeps_its_flags_as_its_source_publishes_them() {
    assert_eq!(
        object(6)["entries"][0]["flags"],
        serde_json::json!({"Sn": "5"})
    );
}

#[test]
fn the_record_lists_its_names_apart_from_its_designations() {
    assert_eq!(
        object(11)["names"],
        serde_json::json!([{"name": "Canis Major", "source": "mcconnachie-2012"}])
    );
    // A researched star is named by its file.
    assert_eq!(
        object(6)["names"],
        serde_json::json!([{"name": "HIP 1 (SIMULATED)", "source": "researched-star-distances"}])
    );
}

#[test]
fn a_designation_in_its_catalogues_form_resolves_as_a_name_does() {
    let db = db();
    for name in ["HIP 1", "hip 1", " HIP 1 "] {
        let (response, record) = read(&db, "/v1/objects", &format!("name={name}"));
        assert_eq!(response.status, 200, "{name}: {record}");
        assert_eq!(record["id"], 6, "{name}");
    }
    let (response, _) = read(&db, "/v1/objects", "name=HIP 999999");
    assert_eq!(response.status, 404);
}
