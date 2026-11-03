//! End-to-end tests of the Worker inside `wrangler dev`, over the fixture state. They are
//! ignored so that `cargo test` skips them; `cargo xtask integration` starts the Worker and runs
//! them with `UNIV_E2E_URL` naming it.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::sync::LazyLock;

use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode};
use serde_json::Value;

/// The Worker's base URL, as `cargo xtask integration` gives it.
static BASE: LazyLock<String> = LazyLock::new(|| {
    std::env::var("UNIV_E2E_URL")
        .expect("UNIV_E2E_URL names the Worker: run `cargo xtask integration`")
});

/// One client for every test, which follows no redirect so that a test sees it.
static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
});

fn url(path: &str) -> String {
    format!("{}{path}", *BASE)
}

fn get(path: &str) -> Response {
    CLIENT.get(url(path)).send().unwrap()
}

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response
        .headers()
        .get(name)
        .map(|value| value.to_str().unwrap())
}

fn json(response: Response) -> Value {
    serde_json::from_slice(&response.bytes().unwrap()).unwrap()
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_read_with_no_authorization_header_succeeds_from_any_origin() {
    let response = CLIENT
        .get(url("/v1/openapi.json"))
        .header("Origin", "https://example.org")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header(&response, "access-control-allow-origin"), Some("*"));
    let document = json(response);
    assert_eq!(document["openapi"], "3.1.0");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_cors_preflight_for_a_read_is_answered_without_credentials() {
    let response = CLIENT
        .request(Method::OPTIONS, url("/v1/openapi.json"))
        .header("Origin", "https://example.org")
        .header("Access-Control-Request-Method", "GET")
        .header("Access-Control-Request-Headers", "authorization")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(header(&response, "access-control-allow-origin"), Some("*"));
    let allowed = header(&response, "access-control-allow-headers").unwrap();
    assert!(allowed.to_ascii_lowercase().contains("authorization"));
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn an_unknown_route_answers_404_with_problem_details() {
    let response = get("/v1/nowhere");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        header(&response, "content-type"),
        Some("application/problem+json")
    );
    let problem = json(response);
    assert_eq!(problem["type"], "urn:univ:problem:route-not-found");
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["path"], "/v1/nowhere");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_head_request_is_answered_with_no_body() {
    // An HTTP client never reads a HEAD's body, so the bytes on the wire are read instead.
    for (path, status) in [("/v1/openapi.json", "200"), ("/v1/nowhere", "404")] {
        let raw = raw_head(path);
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with(&format!("HTTP/1.1 {status}")), "{head}");
        assert!(
            head.to_ascii_lowercase()
                .contains("access-control-allow-origin: *"),
            "{head}"
        );
        assert_eq!(body, "", "HEAD {path} carried a body");
    }
}

/// Everything the Worker sends back to a HEAD of `path`, read until it closes.
fn raw_head(path: &str) -> String {
    let authority = BASE.trim_start_matches("http://");
    let mut stream = std::net::TcpStream::connect(authority).unwrap();
    let request = format!("HEAD {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    std::io::Write::write_all(&mut stream, request.as_bytes()).unwrap();
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut stream, &mut raw).unwrap();
    raw
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn the_releases_are_listed_newest_first_with_the_pruned_one_and_latest_named() {
    let listing = json(get("/v1/releases"));
    assert_eq!(listing["latest"], 2);
    let releases = listing["releases"].as_array().unwrap();
    assert_eq!(releases[0]["number"], 2);
    assert_eq!(releases[0]["pruned_at"], Value::Null);
    assert_eq!(releases[1]["number"], 1);
    assert!(releases[1]["pruned_at"].is_string());
    assert_eq!(releases[0]["cosmology"]["hubble_constant"], 67.66);
    assert!(releases[0]["counts"]["objects"].as_u64().unwrap() > 0);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn two_reads_pinned_to_one_release_return_identical_data() {
    let path = "/v1/structures/bootes-void?release=2";
    let first = get(path);
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(header(&first, "univ-release"), Some("2"));
    let first = first.bytes().unwrap();
    let second = get(path).bytes().unwrap();
    assert_eq!(first, second);
    let unpinned = get("/v1/structures/bootes-void").bytes().unwrap();
    assert_eq!(first, unpinned, "latest is release 2");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_pruned_release_answers_410_and_one_never_published_404() {
    let pruned = get("/v1/sources?release=1");
    assert_eq!(pruned.status(), StatusCode::GONE);
    let problem = json(pruned);
    assert_eq!(problem["type"], "urn:univ:problem:release-pruned");
    assert!(problem["pruned_at"].is_string());
    let missing = get("/v1/sources?release=99");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(missing)["type"], "urn:univ:problem:release-not-found");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_source_gives_its_terms_query_and_provenance() {
    let listing = json(get("/v1/sources"));
    let ids: Vec<&str> = listing["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"hipparcos-2007"), "{ids:?}");
    let source = json(get("/v1/sources/hipparcos-2007"));
    assert_eq!(source["release"], 2);
    assert_eq!(source["query"]["table"], "I/311/hip2");
    assert!(
        source["terms"]["credit"]
            .as_str()
            .unwrap()
            .contains("VizieR")
    );
    let provenance = json(get("/v1/sources/hipparcos-2007/provenance"));
    let counts = &provenance["counts"];
    assert_eq!(
        counts["fetched"].as_u64().unwrap(),
        counts["published"].as_u64().unwrap() + counts["left_out"].as_u64().unwrap()
    );
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn structures_are_listed_in_pages_and_read_with_their_citations() {
    let page = json(get("/v1/structures?kind=void"));
    let structures = page["structures"].as_array().unwrap();
    assert!(structures.iter().all(|s| s["kind"] == "void"));
    assert!(structures.iter().any(|s| s["id"] == "bootes-void"));
    let bootes = json(get("/v1/structures/bootes-void"));
    assert_eq!(
        bootes["record"]["papers"][0]["bibcode"],
        "1987ApJ...314..493K"
    );
    assert_eq!(bootes["centre"]["unit"], "megaparsec");
    let missing = get("/v1/structures/nowhere");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_source_gives_its_primary_reference_and_coverage_from_schema_v3() {
    let source = json(get("/v1/sources/hipparcos-2007"));
    assert_eq!(source["reference"]["bibcode"], "2007A&A...474..653V");
    let sky = &source["coverage"]["sky"];
    assert_eq!(sky["order"], 5);
    assert_ne!(sky["cells"], Value::Array(Vec::new()), "no cells");
    let radial = source["coverage"]["radial"].as_array().unwrap();
    assert!(
        radial.iter().any(|r| r["measure"] == "parallax"),
        "{radial:?}"
    );
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_size_published_under_h0_50_is_served_beside_its_rescaled_value() {
    let bootes = json(get("/v1/structures/bootes-void"));
    let radius = bootes["rescaled"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["native"]["value"] == 63.0)
        .unwrap_or_else(|| panic!("no rescaled 63 Mpc: {}", bootes["rescaled"]));
    assert_eq!(radius["native"]["hubble_constant"], 50.0);
    assert_eq!(radius["method"], "hubble_ratio");
    let rescaled = radius["rescaled"]["value"].as_f64().unwrap();
    assert!((rescaled - 63.0 * 50.0 / 67.66).abs() < 1e-6, "{rescaled}");
    assert!(bootes["shape"].is_object());
    assert!(bootes["derivation"]["from"].is_array());
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn an_object_is_read_by_id_by_name_and_by_designation_through_the_worker() {
    let by_id = json(get("/v1/objects/11"));
    assert_eq!(by_id["entries"][0]["identifier"], "Canis Major");
    assert_eq!(by_id["position"]["frame"], "ICRS equatorial Cartesian");
    assert!(by_id["preferred"]["rule"].is_string());
    let by_name = get("/v1/objects?name=canis%20MAJOR");
    assert_eq!(by_name.status(), StatusCode::OK);
    assert_eq!(json(by_name)["id"], 11);
    let by_designation = json(get(
        "/v1/objects?source=mcconnachie-2012&designation=Sagittarius%20dSph",
    ));
    assert_eq!(by_designation["id"], 12);
    let missing = get("/v1/objects/99999");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(missing)["looked_up"]["id"], 99_999);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn every_tile_of_every_scale_domain_matches_the_sha256_its_manifest_gives() {
    use sha2::{Digest, Sha256};
    let domains = json(get("/v1/scale-domains?release=2"));
    let domains = domains["domains"].as_array().unwrap();
    assert_ne!(domains.len(), 0);
    let mut tiles = 0;
    for domain in domains {
        let name = domain["name"].as_str().unwrap();
        let manifest = get(&format!("/v1/scale-domains/{name}/manifest?release=2"));
        assert_eq!(manifest.status(), StatusCode::OK);
        let manifest = json(manifest);
        assert_eq!(manifest["frame"]["name"], "ICRS equatorial Cartesian");
        for node in manifest["nodes"].as_array().unwrap() {
            let address = node["address"].as_str().unwrap();
            let tile = get(&format!(
                "/v1/scale-domains/{name}/tiles/{address}?release=2"
            ));
            assert_eq!(tile.status(), StatusCode::OK, "{name}/{address}");
            assert_eq!(
                header(&tile, "cache-control"),
                Some("public, max-age=31536000, immutable")
            );
            let bytes = tile.bytes().unwrap();
            let sha256 = Sha256::digest(&bytes)
                .iter()
                .fold(String::new(), |mut hex, b| {
                    use std::fmt::Write;
                    let _ = write!(hex, "{b:02x}");
                    hex
                });
            assert_eq!(sha256, node["sha256"].as_str().unwrap(), "{name}/{address}");
            tiles += 1;
        }
    }
    assert_ne!(tiles, 0);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_tile_read_unpinned_lands_on_the_tile_of_the_latest_release() {
    let domains = json(get("/v1/scale-domains"));
    let name = domains["domains"][0]["name"].as_str().unwrap().to_owned();
    let manifest = json(get(&format!("/v1/scale-domains/{name}/manifest")));
    let address = manifest["nodes"][0]["address"].as_str().unwrap();
    let redirect = get(&format!("/v1/scale-domains/{name}/tiles/{address}"));
    assert_eq!(redirect.status(), StatusCode::FOUND);
    assert_eq!(header(&redirect, "cache-control"), Some("no-store"));
    let location = header(&redirect, "location").unwrap().to_owned();
    assert!(location.ends_with("?release=2"), "{location}");
    assert_eq!(get(&location).status(), StatusCode::OK);
    let missing = get(&format!("/v1/scale-domains/{name}/tiles/nowhere"));
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let unknown = json(get("/v1/scale-domains/nowhere/manifest"));
    assert_ne!(unknown["scale_domains"].as_array().unwrap().len(), 0);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn the_grids_are_listed_and_an_unknown_grid_answers_404() {
    let listing = get("/v1/grids");
    assert_eq!(listing.status(), StatusCode::OK);
    assert!(json(listing)["grids"].is_array());
    let missing = get("/v1/grids/nowhere");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(missing)["id"], "nowhere");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn the_tile_format_describes_each_encoding_the_release_uses() {
    let format = json(get("/v1/tile-format"));
    assert_eq!(format["release"], 2);
    assert_eq!(format["encodings"][0]["version"], 1);
    assert!(format["manifest_schema"]["properties"]["nodes"].is_object());
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_tile_manifest_names_its_frame_unit_cosmology_nodes_rotations_sources_and_encoding() {
    let domains = json(get("/v1/scale-domains?release=2"));
    for domain in domains["domains"].as_array().unwrap() {
        let name = domain["name"].as_str().unwrap();
        let m = json(get(&format!("/v1/scale-domains/{name}/manifest?release=2")));
        assert_eq!(m["frame"]["name"], "ICRS equatorial Cartesian", "{name}");
        assert_eq!(m["frame"]["origin"], "Solar System barycentre");
        assert!(m["unit"].is_string());
        assert_eq!(m["cosmology"]["hubble_constant"], 67.66);
        for node in m["nodes"].as_array().unwrap() {
            for field in ["address", "bounds", "level", "points", "bytes", "sha256"] {
                assert!(!node[field].is_null(), "{name}: a node gives its {field}");
            }
        }
        for rotation in ["to_galactic", "to_supergalactic"] {
            assert_eq!(m["rotations"][rotation].as_array().unwrap().len(), 3);
        }
        for source in m["sources"].as_array().unwrap() {
            assert!(source["credit"].is_string(), "{name}: {source}");
        }
        assert_eq!(m["encoding"]["version"], 1);
        for level in m["levels"].as_array().unwrap() {
            assert!(level["step"].as_f64().unwrap() > 0.0);
        }
    }
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_head_of_a_tile_gives_its_size_and_no_body() {
    let domains = json(get("/v1/scale-domains?release=2"));
    let name = domains["domains"][0]["name"].as_str().unwrap().to_owned();
    let manifest = json(get(&format!("/v1/scale-domains/{name}/manifest?release=2")));
    let node = &manifest["nodes"][0];
    let path = format!(
        "/v1/scale-domains/{name}/tiles/{}?release=2",
        node["address"].as_str().unwrap()
    );
    let raw = raw_head(&path);
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let length = format!("content-length: {}", node["bytes"]);
    assert!(head.to_ascii_lowercase().contains(&length), "{head}");
    assert_eq!(body, "");
}
