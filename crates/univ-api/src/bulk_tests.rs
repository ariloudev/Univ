//! Scale domains, tile manifests, tiles and volume grids (fetch-a-tile-manifest,
//! fetch-point-tiles, fetch-a-volume-grid), from the test kit's release 2 and a bucket in
//! memory.

use serde_json::Value;

use crate::http::{Body, Method, Request};
use crate::testkit::{COSMIC_WEB_MANIFEST, GRIDS, RELEASES, SOURCES, STRUCTURES, Sqlite};

/// A grid header as ingest writes it, cut to the keys it names: field 0 published with an
/// uncertainty, field 1 without.
const GRID_HEADER: &str = r#"{"id":"cf4-basins","fields":[
  {"samples":"files/f00000",
   "uncertainty":{"form":"standard_deviation","samples":"files/f0510a"}},
  {"samples":"files/f10000","uncertainty":{"form":"none_reported"}}]}"#;

fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES).exec(STRUCTURES).exec(GRIDS);
    db.put("files/c05a1c", COSMIC_WEB_MANIFEST)
        .put("files/aaaa", b"abc".to_vec())
        .put("files/bbbb", b"de".to_vec())
        .put("files/9d1dea", GRID_HEADER)
        .put("files/f10000", vec![0_u8; 8]);
    db
}

#[test]
fn the_listing_holds_each_scale_domain_with_its_radius_in_its_own_unit() {
    let (response, listing) = db().get_json("/v1/scale-domains", "");
    assert_eq!(response.status, 200);
    let domains = listing["domains"].as_array().unwrap();
    let names: Vec<&Value> = domains.iter().map(|d| &d["name"]).collect();
    assert_eq!(names, ["cosmic_web", "stars"]);
    assert_eq!(domains[0]["unit"], "megaparsec");
    assert_eq!(domains[0]["radius"], 168.0);
    assert_eq!(domains[1]["unit"], "parsec");
    assert_eq!(
        domains[0]["manifest"],
        "/v1/scale-domains/cosmic_web/manifest?release=2"
    );
}

#[test]
fn a_tile_manifest_is_served_from_the_bucket_as_published() {
    let db = db();
    let response = db.get("/v1/scale-domains/cosmic_web/manifest", "release=2");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, Body::Object("files/c05a1c".into()));
    assert_eq!(db.bytes_of(&response), COSMIC_WEB_MANIFEST.as_bytes());
    assert_eq!(
        response.header("cache-control"),
        Some(crate::releases::IMMUTABLE)
    );
    let unpinned = db.get("/v1/scale-domains/cosmic_web/manifest", "");
    assert_eq!(unpinned.header("cache-control"), Some("no-cache"));
}

#[test]
fn an_unknown_scale_domain_answers_404_listing_the_domains_the_release_has() {
    let (response, problem) = db().get_json("/v1/scale-domains/galaxies/manifest", "");
    assert_eq!(response.status, 404);
    assert_eq!(problem["type"], "urn:univ:problem:scale-domain-not-found");
    assert_eq!(
        problem["scale_domains"],
        serde_json::json!(["cosmic_web", "stars"])
    );
}

#[test]
fn a_tile_is_streamed_from_the_bucket_marked_immutable_for_a_year() {
    let db = db();
    let response = db.get("/v1/scale-domains/cosmic_web/tiles/r07", "release=2");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        Body::Object("files/bbbb".into()),
        "the Worker hands the object to the runtime, unread"
    );
    assert_eq!(db.bytes_of(&response), b"de");
    assert_eq!(
        response.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(response.header("etag"), Some("\"bbbb\""));
    assert_eq!(response.header("univ-release"), Some("2"));
}

#[test]
fn a_tile_read_unpinned_is_redirected_to_the_tile_of_the_latest_release() {
    let response = db().get("/v1/scale-domains/cosmic_web/tiles/r", "");
    assert_eq!(response.status, 302);
    assert_eq!(
        response.header("location"),
        Some("/v1/scale-domains/cosmic_web/tiles/r?release=2")
    );
    assert_eq!(response.header("cache-control"), Some("no-store"));
}

#[test]
fn a_node_the_manifest_does_not_hold_answers_404_naming_the_address() {
    let (response, problem) = db().get_json("/v1/scale-domains/cosmic_web/tiles/r99", "release=2");
    assert_eq!(response.status, 404);
    assert_eq!(problem["type"], "urn:univ:problem:tile-not-found");
    assert_eq!(problem["address"], "r99");
}

#[test]
fn a_head_of_a_tile_leaves_its_object_to_the_runtime_with_no_made_up_length() {
    let mut request = Request::new(Method::Head, "/v1/scale-domains/cosmic_web/tiles/r");
    "release=2".clone_into(&mut request.query);
    let response = db().answer(&request);
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        Body::Object("files/aaaa".into()),
        "the Worker answers a HEAD of an object with the object's size, unread"
    );
    assert_eq!(response.header("content-length"), None);
}

#[test]
fn each_listed_grid_gives_its_id_quantities_unit_shape_and_source() {
    let (response, listing) = db().get_json("/v1/grids", "");
    assert_eq!(response.status, 200);
    let grid = &listing["grids"][0];
    assert_eq!(grid["id"], "cf4-basins");
    assert_eq!(grid["source"], "researched-structures");
    assert_eq!(grid["unit"], "megaparsec_over_h");
    assert_eq!(grid["shape"], serde_json::json!([128, 128, 128]));
    assert_eq!(grid["fields"][1]["field"], "basin_label");
    assert_eq!(grid["fields"][1]["kind"], "reconstructed");
    assert_eq!(
        grid["fields"][1]["samples"],
        "/v1/grids/cf4-basins/fields/1?release=2"
    );
}

#[test]
fn a_grid_header_and_its_samples_are_served_from_the_bucket_as_published() {
    let db = db();
    let header = db.get("/v1/grids/cf4-basins", "release=2");
    assert_eq!(
        header.status,
        200,
        "{:?}",
        String::from_utf8_lossy(&db.bytes_of(&header))
    );
    assert_eq!(db.bytes_of(&header), GRID_HEADER.as_bytes());
    let samples = db.get("/v1/grids/cf4-basins/fields/1", "release=2");
    assert_eq!(samples.status, 200);
    assert_eq!(samples.body, Body::Object("files/f10000".into()));
    assert_eq!(
        samples.header("content-type"),
        Some("application/octet-stream")
    );
}

#[test]
fn an_unknown_grid_or_field_answers_404_naming_it() {
    let db = db();
    let (response, problem) = db.get_json("/v1/grids/nowhere", "");
    assert_eq!(response.status, 404);
    assert_eq!(problem["type"], "urn:univ:problem:grid-not-found");
    assert_eq!(problem["id"], "nowhere");
    for field in ["2", "x", "-1"] {
        let (response, problem) = db.get_json(&format!("/v1/grids/cf4-basins/fields/{field}"), "");
        assert_eq!(response.status, 404, "{field}");
        assert_eq!(problem["type"], "urn:univ:problem:grid-field-not-found");
        assert_eq!(problem["field"], field);
    }
    let (response, _) = db.get_json("/v1/grids/cf4-basins/fields/1/uncertainty", "");
    assert_eq!(response.status, 404, "field 1 has no uncertainty");
}

#[test]
fn the_tile_format_gives_the_byte_layout_of_each_encoding_the_release_uses() {
    let db = db();
    db.put("files/57a125", r#"{"encoding":{"version":1},"nodes":[]}"#);
    let (response, format) = db.get_json("/v1/tile-format", "");
    assert_eq!(response.status, 200, "{format}");
    let encodings = format["encodings"].as_array().unwrap();
    assert_eq!(encodings.len(), 1, "both domains use version 1");
    assert_eq!(encodings[0]["version"], 1);
    assert!(encodings[0]["header_bytes"].as_u64().unwrap() > 0);
    assert!(
        format["manifest_schema"]["properties"]["nodes"].is_object(),
        "{format}"
    );
}

#[test]
fn a_fields_uncertainty_is_served_where_it_was_published() {
    let db = db();
    let (_, listing) = db.get_json("/v1/grids", "");
    let fields = &listing["grids"][0]["fields"];
    assert_eq!(
        fields[0]["uncertainty"],
        "/v1/grids/cf4-basins/fields/0/uncertainty?release=2"
    );
    assert_eq!(fields[1]["uncertainty"], Value::Null);
    let response = db.get("/v1/grids/cf4-basins/fields/0/uncertainty", "release=2");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, Body::Object("files/f0510a".into()));
}

#[test]
fn a_read_of_reference_data_with_a_valid_key_returns_what_the_same_read_without_one_does() {
    let db = db();
    let key = db.key();
    for path in [
        "/v1/releases",
        "/v1/sources",
        "/v1/structures",
        "/v1/scale-domains",
        "/v1/scale-domains/cosmic_web/manifest",
        "/v1/grids",
    ] {
        let anonymous = db.answer(&Request::new(Method::Get, path));
        let keyed = db.answer(&Request::new(Method::Get, path).with_bearer(&key));
        assert_eq!(anonymous.status, 200, "{path}");
        assert_eq!(keyed.status, anonymous.status, "{path}");
        for header in ["content-type", "univ-release", "etag", "cache-control"] {
            assert_eq!(
                keyed.header(header),
                anonymous.header(header),
                "{path}: {header}"
            );
        }
        assert_eq!(db.bytes_of(&keyed), db.bytes_of(&anonymous), "{path}");
    }
}
