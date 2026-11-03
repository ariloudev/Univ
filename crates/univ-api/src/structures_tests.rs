//! The structures of a release (read-a-structure), read from the test kit's release 2.

use serde_json::{Value, json};

use crate::http::Response;
use crate::testkit::{MANY_STRUCTURES, RELEASES, STRUCTURES, Sqlite, body};

fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(STRUCTURES);
    db
}

fn get(db: &Sqlite, path: &str, query: &str) -> Response {
    db.get(path, query)
}

fn read(db: &Sqlite, path: &str, query: &str) -> Value {
    let response = get(db, path, query);
    assert_eq!(response.status, 200, "{}", body(&response));
    body(&response)
}

fn structure(id: &str) -> Value {
    read(&db(), &format!("/v1/structures/{id}"), "")
}

#[test]
fn the_geometry_is_a_univ_model_geometry_in_a_named_frame() {
    let bootes = structure("bootes-void");
    assert_eq!(bootes["shape"]["geometry"]["kind"], "primitive");
    assert_eq!(bootes["shape"]["geometry"]["primitive"]["form"], "sphere");
    assert_eq!(bootes["shape"]["frame"], "icrs");
    assert_eq!(bootes["shape"]["unit"], "megaparsec");
    assert_eq!(bootes["centre"]["frame"], "ICRS equatorial Cartesian");
    assert_eq!(bootes["centre"]["origin"], "Solar System barycentre");
    assert_eq!(bootes["centre"]["unit"], "megaparsec");
}

#[test]
fn each_published_number_cites_its_source_and_where_it_was_taken_from() {
    let radius = &structure("bootes-void")["record"]["values"]["radius"];
    assert_eq!(radius["cite"]["paper"], "kirshner-1987");
    assert_eq!(radius["cite"]["locator"]["form"], "sentence");
    assert_eq!(radius["cite"]["locator"]["page"], 501);
}

#[test]
fn each_number_derived_from_a_published_number_names_its_derivation() {
    let derivation = &structure("bootes-void")["derivation"];
    let step = &derivation["frame_chain"][0];
    assert_eq!(step["step"], "to_icrs");
    assert_eq!(step["from"], "fk4_b1950");
    assert!(step["citation"].as_str().unwrap().contains("Standish 1982"));
    assert!(
        derivation["distance"]["method"]
            .as_str()
            .unwrap()
            .contains("release cosmology")
    );
}

#[test]
fn a_structure_outlined_from_a_reconstruction_has_the_kind_of_value_reconstructed() {
    let local = structure("local-void");
    assert_eq!(local["status"], "reconstructed");
    assert_eq!(local["record"]["values"]["depth"]["kind"], "reconstructed");
}

#[test]
fn a_number_digitised_from_a_figure_has_the_kind_of_value_estimated() {
    let edge = &structure("local-void")["record"]["values"]["edge"];
    assert_eq!(edge["cite"]["locator"]["form"], "figure");
    assert_eq!(edge["kind"], "estimated");
}

#[test]
fn each_member_is_an_object_id_or_a_tracer_with_its_position_and_kind() {
    let members = &structure("giant-grb-ring")["members"]["shape"]["members"];
    assert_eq!(members[0]["member"], "tracer");
    assert_eq!(members[0]["kind"], "gamma-ray burst");
    assert_eq!(members[0]["at"], json!([1.0, 2.0, 3.0]));
    assert_eq!(members[1]["member"], "object");
    assert_eq!(members[1]["id"], 42);
}

#[test]
fn a_structure_outlined_from_a_grid_names_the_grid_threshold_and_test() {
    let outline = &structure("local-void")["outline"];
    assert_eq!(outline["grid"], "cf4pp-density");
    assert_eq!(outline["threshold"]["chosen"]["chosen"], -1.0);
    assert_eq!(
        outline["test"]["rule"],
        "underdense_in_every_covering_catalogue"
    );
    assert_eq!(outline["verdict"], "refused");
}

#[test]
fn a_size_published_for_another_hubble_constant_gives_its_native_and_rescaled_values() {
    let rescaled = &structure("bootes-void")["rescaled"][0];
    assert_eq!(rescaled["native"]["value"], 63.0);
    assert_eq!(rescaled["native"]["hubble_constant"], 50.0);
    let value = rescaled["rescaled"]["value"].as_f64().unwrap();
    assert!((value - 63.0 * 50.0 / 67.66).abs() < 1e-9, "{value}");
    assert_eq!(rescaled["method"], "hubble_ratio");
}

#[test]
fn a_structure_that_cannot_be_placed_has_no_centre() {
    assert_eq!(structure("giant-grb-ring")["centre"], Value::Null);
    assert_eq!(structure("giant-grb-ring")["rescaled"], json!([]));
}

#[test]
fn each_listed_structure_gives_its_id_name_kind_scale_domain_and_centre() {
    let page = read(&db(), "/v1/structures", "");
    assert_eq!(page["release"], 2);
    let first = &page["structures"][0];
    assert_eq!(first["id"], "bootes-void");
    assert_eq!(first["name"], "Bootes Void");
    assert_eq!(first["kind"], "void");
    assert_eq!(first["scale_domain"], "cosmic_web");
    assert_eq!(first["centre"]["x"], -115.49);
    assert_eq!(page["next"], Value::Null);
}

#[test]
fn the_listing_is_filtered_by_the_kinds_and_scale_domains_named() {
    let voids = read(&db(), "/v1/structures", "kind=void");
    let ids: Vec<&Value> = voids["structures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["id"])
        .collect();
    assert_eq!(ids, ["bootes-void", "local-void"]);
    let both = read(
        &db(),
        "/v1/structures",
        "kind=void,ring&scale_domain=cosmic_web",
    );
    assert_eq!(both["structures"].as_array().unwrap().len(), 3);
    let none = read(&db(), "/v1/structures", "scale_domain=stars");
    assert_eq!(none["structures"], json!([]));
}

#[test]
fn the_listing_comes_in_pages_of_at_most_500_with_a_cursor_to_the_next() {
    let db = db();
    db.exec(MANY_STRUCTURES);
    let mut seen = Vec::new();
    let mut query = String::new();
    loop {
        let page = read(&db, "/v1/structures", &query);
        let structures = page["structures"].as_array().unwrap();
        assert!(structures.len() <= 500);
        seen.extend(
            structures
                .iter()
                .map(|s| s["id"].as_str().unwrap().to_owned()),
        );
        match page["next"].as_str() {
            Some(cursor) => query = format!("cursor={cursor}"),
            None => break,
        }
    }
    assert_eq!(seen.len(), 1_203);
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, seen, "pages hold each structure once, in id order");
}

#[test]
fn a_cursor_keeps_reading_its_release() {
    let db = db();
    db.exec(MANY_STRUCTURES);
    let first = read(&db, "/v1/structures", "");
    let cursor = first["next"].as_str().unwrap().to_owned();
    assert!(cursor.starts_with("2:"));
    let second = read(&db, "/v1/structures", &format!("cursor={cursor}&release=1"));
    assert_eq!(
        second["release"], 2,
        "the cursor's release, not the parameter's"
    );
    let pruned = get(&db, "/v1/structures", "cursor=1:filler-0000");
    assert_eq!(
        pruned.status, 410,
        "a cursor into a pruned release is refused"
    );
}

#[test]
fn a_malformed_cursor_is_refused_naming_it() {
    let response = get(&db(), "/v1/structures", "cursor=nonsense");
    assert_eq!(response.status, 400);
    assert_eq!(body(&response)["parameter"], "cursor");
}

#[test]
fn an_unknown_structure_answers_404_naming_it() {
    let response = get(&db(), "/v1/structures/nowhere", "");
    assert_eq!(response.status, 404);
    let problem = body(&response);
    assert_eq!(problem["type"], "urn:univ:problem:structure-not-found");
    assert_eq!(problem["id"], "nowhere");
}
