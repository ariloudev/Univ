//! The sources of a release (read-a-source), read from the test kit's release 2.

use serde_json::{Value, json};

use crate::http::Response;
use crate::testkit::{RELEASES, SOURCES, Sqlite, body};

fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES);
    db
}

fn get(db: &Sqlite, path: &str) -> Response {
    db.get(path, "")
}

fn read(path: &str) -> Value {
    let response = get(&db(), path);
    assert_eq!(response.status, 200, "{}", body(&response));
    assert_eq!(response.header("univ-release"), Some("2"));
    body(&response)
}

#[test]
fn each_listed_source_gives_its_id_title_scale_domain_and_published_entries() {
    let listing = read("/v1/sources");
    assert_eq!(listing["release"], 2);
    let first = &listing["sources"][0];
    assert_eq!(first["id"], "hipparcos-2007");
    assert_eq!(first["title"], "Hipparcos, the new reduction");
    assert_eq!(first["scale_domain"], "stars");
    assert_eq!(first["entries"], 117_955);
    assert_eq!(listing["sources"].as_array().unwrap().len(), 2);
}

#[test]
fn each_listed_source_gives_its_citation_and_credit() {
    let first = &read("/v1/sources")["sources"][0];
    assert!(first["citation"].as_str().unwrap().contains("A&A 474, 653"));
    assert_eq!(first["credit"], "This research has made use of VizieR.");
}

#[test]
fn the_citation_gives_the_bibcode_or_doi_of_the_primary_reference() {
    let source = read("/v1/sources/hipparcos-2007");
    assert_eq!(source["reference"]["bibcode"], "2007A&A...474..653V");
    assert_eq!(source["reference"]["doi"], "10.1051/0004-6361:20078357");
    let compilation = read("/v1/sources/researched-structures");
    assert_eq!(compilation["reference"]["bibcode"], Value::Null);
    assert!(
        compilation["reference"]["note"]
            .as_str()
            .unwrap()
            .contains("compilation")
    );
}

#[test]
fn the_terms_of_use_and_credit_are_given_as_the_definition_states_them() {
    let terms = &read("/v1/sources/hipparcos-2007")["terms"];
    assert_eq!(
        terms["url"],
        "https://cds.unistra.fr/vizier-org/licences_vizier.html"
    );
    assert_eq!(
        terms["quote"],
        "The data are free of usage in a scientific context."
    );
    assert_eq!(terms["credit"], "This research has made use of VizieR.");
    assert_eq!(terms["redistribution"], "permitted_with_credit");
}

#[test]
fn the_terms_give_the_redistribution_status_and_any_non_commercial_clause() {
    let db = db();
    let permitted = body(&get(&db, "/v1/sources/hipparcos-2007"));
    // Terms that allow commercial use and grant redistribution carry neither mark.
    assert_eq!(
        permitted["terms"]["redistribution"],
        "permitted_with_credit"
    );
    assert!(permitted["terms"].get("non_commercial").is_none());
    assert!(permitted["terms"].get("permission").is_none());
    db.exec(
        "UPDATE sources SET redistribution = 'permission_pending' WHERE release = 2 AND n = 0; \
         INSERT INTO source_terms VALUES (2, 0, 'Tabular data are under CC-BY-NC licence', \
         '{\"asked_of\":\"WFAU, mar@roe.ac.uk\",\"asked_on\":\"2026-11-05\"}', NULL, NULL);",
    );
    let pending = body(&get(&db, "/v1/sources/hipparcos-2007"));
    let terms = &pending["terms"];
    assert_eq!(terms["redistribution"], "permission_pending");
    assert_eq!(
        terms["non_commercial"],
        "Tabular data are under CC-BY-NC licence"
    );
    assert_eq!(
        terms["permission"],
        json!({"asked_of": "WFAU, mar@roe.ac.uk", "asked_on": "2026-11-05"})
    );
    assert!(terms.get("permission_to_ask").is_none());
    // A request not yet sent names whom it is for, and no date.
    db.exec(
        "UPDATE source_terms SET permission = '{\"asked_of\":\"WFAU, mar@roe.ac.uk\"}' \
         WHERE release = 2 AND source = 0;",
    );
    let unsent = body(&get(&db, "/v1/sources/hipparcos-2007"));
    assert!(unsent["terms"].get("permission").is_none());
    assert_eq!(
        unsent["terms"]["permission_to_ask"],
        json!({"asked_of": "WFAU, mar@roe.ac.uk"})
    );
}

#[test]
fn the_query_and_the_cuts_are_the_ones_the_fetch_ran_verbatim() {
    let source = read("/v1/sources/hipparcos-2007");
    assert_eq!(
        source["query"],
        json!({"method": "vizier_tsv", "table": "I/311/hip2", "constraints": [],
               "slices": {"column": "HIP", "from": 1, "to": 120_404, "step": 30_000},
               "rows": 117_955})
    );
    assert_eq!(
        source["cuts"],
        json!(["No server-side cut: every row of I/311/hip2."])
    );
}

#[test]
fn the_coverage_gives_the_sky_footprint_and_the_range_of_each_radial_measure() {
    let coverage = &read("/v1/sources/hipparcos-2007")["coverage"];
    assert_eq!(coverage["sky"], json!({"order": 5, "cells": [1, 2, 3]}));
    let parallax = &coverage["radial"][0];
    assert_eq!(parallax["measure"], "parallax");
    assert_eq!(parallax["min"], 0.1);
    assert_eq!(parallax["max"], 772.33);
    assert_eq!(parallax["unit"], "milliarcsecond");
    let compilation = &read("/v1/sources/researched-structures")["coverage"];
    assert_eq!(compilation["sky"], Value::Null);
    assert_eq!(compilation["radial"], json!([]));
}

#[test]
fn a_release_written_before_schema_v3_reads_without_reference_or_coverage() {
    let db = db();
    db.exec("DELETE FROM source_details;");
    let response = get(&db, "/v1/sources/hipparcos-2007");
    assert_eq!(response.status, 200);
    let source = body(&response);
    assert_eq!(source["reference"], Value::Null);
    assert_eq!(source["coverage"], Value::Null);
    assert_eq!(
        source["terms"]["credit"],
        "This research has made use of VizieR."
    );
}

#[test]
fn each_acquired_payload_is_listed_with_its_sha256_and_retrieval_time() {
    let payloads = &read("/v1/sources/hipparcos-2007/provenance")["payloads"];
    assert_eq!(payloads.as_array().unwrap().len(), 2);
    assert_eq!(payloads[0]["sha256"], "aa11");
    assert_eq!(payloads[0]["retrieved"], "2026-11-01T09:00:00Z");
    assert_eq!(payloads[1]["rows"], Value::Null);
}

#[test]
fn the_record_gives_the_entries_published_and_fetched() {
    let counts = &read("/v1/sources/hipparcos-2007/provenance")["counts"];
    assert_eq!(
        counts,
        &json!({"fetched": 117_955, "published": 117_950, "left_out": 5})
    );
}

#[test]
fn the_record_gives_the_entries_left_out_by_reason() {
    let left_out = &read("/v1/sources/hipparcos-2007/provenance")["left_out"];
    assert_eq!(left_out, &json!({"bad_position": 2, "no_parallax": 3}));
}

#[test]
fn each_fix_of_a_known_defect_is_listed_with_its_reason() {
    let fixes = &read("/v1/sources/hipparcos-2007/provenance")["fixes"];
    assert_eq!(
        fixes,
        &json!([{"reason": "RArad and DErad are served in degrees.",
                 "change": "RArad and DErad are read as degrees.",
                 "entries_changed": 117_955}])
    );
}

#[test]
fn a_source_not_normalized_from_fetched_rows_has_no_counts() {
    let record = read("/v1/sources/researched-structures/provenance");
    assert_eq!(record["counts"], Value::Null);
    assert_eq!(record["payloads"], json!([]));
}

#[test]
fn an_unknown_source_answers_404_naming_it() {
    for path in ["/v1/sources/nowhere", "/v1/sources/nowhere/provenance"] {
        let response = get(&db(), path);
        assert_eq!(response.status, 404, "{path}");
        let problem = body(&response);
        assert_eq!(problem["type"], "urn:univ:problem:source-not-found");
        assert_eq!(problem["id"], "nowhere");
    }
}

#[test]
fn a_source_of_a_pruned_release_is_refused_with_410() {
    let response = db().get("/v1/sources/hipparcos-2007", "release=1");
    assert_eq!(response.status, 410);
    assert_eq!(body(&response)["pruned_at"], "2026-11-02T09:00:00Z");
}

#[test]
fn two_reads_pinned_to_one_release_return_identical_data_whatever_was_published_between() {
    let db = db();
    let before = db.get("/v1/sources/hipparcos-2007", "release=2");
    db.exec(
        "UPDATE sources SET retired = 3 WHERE id = 'hipparcos-2007';
         UPDATE releases SET published_at = '2026-11-03T08:00:00Z' WHERE number = 3;
         UPDATE latest SET release = 3 WHERE one = 1;",
    );
    let after = db.get("/v1/sources/hipparcos-2007", "release=2");
    assert_eq!(before.status, 200);
    assert_eq!(before.body, after.body);
    assert_eq!(after.header("univ-release"), Some("2"));
    let unpinned = db.get("/v1/sources/hipparcos-2007", "");
    assert_eq!(
        unpinned.status, 404,
        "latest moved to release 3, which retired the source"
    );
}
