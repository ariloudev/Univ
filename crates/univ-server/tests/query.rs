//! End-to-end tests of bounded volume queries through the Worker under `wrangler dev`, on the
//! fixture release: objects, structures by their placed shapes, a layer item, the tile nodes
//! covering the volume fetched by their addresses, and a malformed volume refused. Run them
//! with `cargo xtask integration`.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use common::{CLIENT, get, header, issue, json, url, with_json};
use reqwest::StatusCode;
use reqwest::blocking::Response;
use serde_json::{Value, json};

fn query(body: &Value, key: Option<&str>) -> Response {
    let request = CLIENT.post(url("/v1/query/volume"));
    let request = match key {
        Some(key) => request.bearer_auth(key),
        None => request,
    };
    with_json(request, body).send().unwrap()
}

fn sphere(centre: [f64; 3], radius: f64) -> Value {
    json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "sphere", "centre": centre, "radius": radius}})
}

fn of_type<'a>(page: &'a Value, kind: &str) -> Vec<&'a Value> {
    page["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"] == kind)
        .collect()
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_volume_query_finds_objects_structures_by_their_shapes_and_tiles_to_fetch() {
    let response = query(&sphere([0.0; 3], 10_000.0), None);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header(&response, "univ-release"), Some("2"));
    let page = json(response);
    assert_eq!(
        of_type(&page, "object").len(),
        // 16 by their catalogues, and HIP 1 by its SIMULATED star file.
        17,
        "every placed object of the fixture release"
    );
    let structures: Vec<&str> = of_type(&page, "structure")
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for placed in [
        "kbc-void",
        "cfa2-great-wall",
        "giant-grb-ring",
        "bootes-void",
    ] {
        assert!(structures.contains(&placed), "{placed} in {structures:?}");
    }
    for unplaced in [
        "hercules-corona-borealis-great-wall",
        "pisces-cetus-supercluster-complex",
    ] {
        assert!(
            !structures.contains(&unplaced),
            "{unplaced} has no placed geometry"
        );
    }
    assert!(
        of_type(&page, "structure")
            .iter()
            .all(|s| s["matched_by"] == "shape")
    );
    let tiles = page["tiles"].as_array().unwrap();
    assert_ne!(tiles.len(), 0);
    for covering in tiles {
        let domain = covering["scale_domain"].as_str().unwrap();
        for node in covering["nodes"].as_array().unwrap() {
            let tile = get(&format!(
                "/v1/scale-domains/{domain}/tiles/{}?release=2",
                node.as_str().unwrap()
            ));
            assert_eq!(tile.status(), StatusCode::OK, "{domain} {node}");
        }
    }
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_structure_meets_a_volume_at_its_edge_with_its_centre_outside_and_an_item_is_found() {
    // The Bootes Void's placed sphere reaches x = -68.9 Mpc; its centre is near x = -115.5,
    // outside this box. The box lies within the KBC void too, and reaches Ho'oleilana.
    let structures = |page: &Value| -> Vec<String> {
        of_type(page, "structure")
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let edge = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [-75.0, -110.0, 160.0], "max": [-70.0, -105.0, 165.0]}});
    assert!(structures(&json(query(&edge, None))).contains(&"bootes-void".to_owned()));
    let beyond = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [-68.0, -110.0, 160.0], "max": [-60.0, -105.0, 165.0]}});
    assert!(!structures(&json(query(&beyond, None))).contains(&"bootes-void".to_owned()));
    let (_, key) = issue("e2e query", "extended");
    let layer = CLIENT.post(url("/v1/layers")).bearer_auth(&key);
    let created = json(
        with_json(layer, &json!({"title": "Volume", "visibility": "private"}))
            .send()
            .unwrap(),
    );
    let layer = created["id"].as_str().unwrap();
    let item = json!({"id": "probe", "title": "A probe", "summary": "A TEST FIXTURE point.",
        "geometry": {"frame": "galactic", "unit": "megaparsec", "geometry": {"kind": "point", "at": [500.0, 0.0, 0.0]}},
        "kind_of_value": "estimated", "sources": [{"title": "A test fixture", "url": "https://example.org/univ-test-fixture"}]});
    let published = with_json(
        CLIENT
            .post(url(&format!("/v1/layers/{layer}/items")))
            .bearer_auth(&key),
        &item,
    )
    .send()
    .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    let near = json!({"volume": {"frame": "galactic", "unit": "megaparsec", "form": "cone",
        "lon": 0.0, "lat": 0.0, "opening_angle": 1.0, "near": 499.0, "far": 501.0}});
    let mine = json(query(&near, Some(&key)));
    assert_eq!(of_type(&mine, "item").len(), 1);
    assert_eq!(
        of_type(&json(query(&near, None)), "item").len(),
        0,
        "a private layer's item"
    );
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_malformed_volume_is_refused_pointing_at_its_fields() {
    let body = json!({"volume": {"frame": "fk4_b1950", "unit": "megaparsec", "form": "cone",
        "lon": 0, "lat": 0, "opening_angle": 200, "near": 0, "far": 1}});
    let response = query(&body, None);
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let problem = json(response);
    assert_eq!(problem["type"], "urn:univ:problem:query-invalid");
    let mut pointers: Vec<&str> = problem["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["pointer"].as_str().unwrap())
        .collect();
    pointers.sort_unstable();
    assert_eq!(pointers, ["/volume/frame", "/volume/opening_angle"]);
}
