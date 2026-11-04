//! Bounded volume queries: their forms, frames and units, what matches, the filters, and the
//! refusal of a malformed volume (query-a-bounded-volume).

use serde_json::{Value, json};
use univ_model::position::{Frame, between};

use crate::layers_fixture::{layer, publish, sn1987a};
use crate::query_fixture::{Placed, STAR, STARS, galaxy, hits, pointers, query, sphere, volume_db};
use crate::testkit::Sqlite;

/// Galaxies about (100, 0, 0) Mpc, one just inside and one just outside 5 Mpc of it, others
/// toward the galactic centre and away from it, and a star 10 pc off.
fn fixture() -> Sqlite {
    let toward_centre = between(Frame::Galactic, Frame::Icrs)
        .rotation()
        .unwrap()
        .apply([50.0, 0.0, 0.0]);
    volume_db(&[
        galaxy(1001, [100.0, 0.0, 0.0]),
        galaxy(1002, [104.99, 0.0, 0.0]),
        galaxy(1003, [105.01, 0.0, 0.0]),
        galaxy(1004, toward_centre),
        galaxy(1005, toward_centre.map(|c| -c)),
        Placed {
            id: 2001,
            kind: STAR,
            domain: STARS,
            at: [10.0, 0.0, 0.0],
        },
    ])
}

fn page(db: &Sqlite, body: &Value) -> Vec<String> {
    let (response, page) = query(db, body);
    assert_eq!(response.status, 200, "{page}");
    hits(&page)
}

#[test]
fn a_volume_is_a_box_a_sphere_or_a_cone_with_a_distance_range() {
    let db = fixture();
    let ball = json!({"volume": sphere([100.0, 0.0, 0.0], 5.0)});
    assert_eq!(page(&db, &ball), ["object:1001", "object:1002"]);
    let boxed = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [99.0, -1.0, -1.0], "max": [106.0, 1.0, 1.0]}});
    assert_eq!(
        page(&db, &boxed),
        ["object:1001", "object:1002", "object:1003"]
    );
    let cone = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "cone",
        "lon": 0.0, "lat": 0.0, "opening_angle": 2.0, "near": 102.0, "far": 110.0}});
    assert_eq!(page(&db, &cone), ["object:1002", "object:1003"]);
}

#[test]
fn a_volume_names_its_frame_and_unit() {
    let db = fixture();
    let galactic = |l: f64| {
        json!({"volume": {"frame": "galactic", "unit": "kiloparsec", "form": "cone",
        "lon": l, "lat": 0.0, "opening_angle": 1.0, "near": 40_000.0, "far": 60_000.0}})
    };
    assert_eq!(page(&db, &galactic(0.0)), ["object:1004"]);
    assert_eq!(page(&db, &galactic(180.0)), ["object:1005"]);
    let near_sun = json!({"volume": {"frame": "supergalactic", "unit": "parsec", "form": "sphere",
        "centre": [0.0, 0.0, 0.0], "radius": 11.0}});
    let (_, answer) = query(&db, &near_sun);
    assert_eq!(hits(&answer), ["object:2001"]);
    assert_eq!(
        answer["results"][0]["position"],
        json!({"frame": "icrs", "unit": "parsec", "at": [10.0, 0.0, 0.0]})
    );
}

#[test]
fn an_object_matches_by_its_derived_position() {
    let db = fixture();
    for (radius, expected) in [
        (4.98, vec!["object:1001"]),
        (5.0, vec!["object:1001", "object:1002"]),
    ] {
        assert_eq!(
            page(&db, &json!({"volume": sphere([100.0, 0.0, 0.0], radius)})),
            expected
        );
    }
}

#[test]
fn a_structure_or_item_matches_when_its_geometry_meets_the_volume_with_its_centre_outside() {
    let db = fixture();
    // The Bootes Void's placed sphere: centre (-115.49, -107.50, 162.22), radius 46.56 Mpc.
    let edge = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [-75.0, -110.0, 160.0], "max": [-70.0, -105.0, 165.0]}});
    let (_, answer) = query(&db, &edge);
    assert_eq!(hits(&answer), ["structure:bootes-void"]);
    assert_eq!(answer["results"][0]["matched_by"], "shape");
    let beyond = json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [-68.0, -110.0, 160.0], "max": [-60.0, -105.0, 165.0]}});
    assert_eq!(page(&db, &beyond), Vec::<String>::new());
    let key = db.key();
    let id = layer(
        &db,
        &key,
        &json!({"title": "Spheres", "visibility": "public"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut item = sn1987a("wide");
    item["geometry"] = json!({"frame": "icrs", "unit": "megaparsec",
        "geometry": {"kind": "primitive", "primitive": {"form": "sphere", "centre": [400.0, 0.0, 0.0], "radius": 10.0}}});
    assert_eq!(publish(&db, &key, &id, &item).0.status, 201);
    assert_eq!(
        page(&db, &json!({"volume": sphere([412.0, 0.0, 0.0], 2.5)})),
        [format!("item:{id}/wide")]
    );
    assert_eq!(
        page(&db, &json!({"volume": sphere([413.0, 0.0, 0.0], 2.5)})),
        Vec::<String>::new()
    );
}

#[test]
fn a_structure_with_no_placed_shape_matches_by_its_members_or_its_bounding_box_and_says_so() {
    let db = fixture();
    let (_, answer) = query(&db, &json!({"volume": sphere([210.0, 210.0, 210.0], 1.0)}));
    assert_eq!(hits(&answer), ["structure:box-only"]);
    assert_eq!(answer["results"][0]["matched_by"], "bounding_box");
    let (_, answer) = query(&db, &json!({"volume": sphere([320.0, 0.0, 0.0], 0.5)}));
    assert_eq!(hits(&answer), ["structure:members-only"]);
    assert_eq!(answer["results"][0]["matched_by"], "members");
    // Between its two members, inside its box, but meeting neither.
    assert_eq!(
        page(&db, &json!({"volume": sphere([310.0, 0.0, 0.0], 1.0)})),
        Vec::<String>::new()
    );
}

#[test]
fn a_private_layers_items_match_only_with_its_owning_key() {
    let db = fixture();
    let (owner, other) = (db.key(), db.key());
    let id = layer(&db, &owner, &json!({"title": "Mine"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    publish(&db, &owner, &id, &sn1987a("sn"));
    // SN 1987A at (1.977, 18.10, -48.07) kpc.
    let body = json!({"volume": {"frame": "icrs", "unit": "kiloparsec", "form": "sphere",
        "centre": [2.0, 18.0, -48.0], "radius": 1.0}});
    assert_eq!(page(&db, &body), Vec::<String>::new());
    let (_, mine) = db.with_key(&owner, crate::query_fixture::request(&body));
    assert_eq!(hits(&mine), [format!("item:{id}/sn")]);
    let (_, theirs) = db.with_key(&other, crate::query_fixture::request(&body));
    assert_eq!(hits(&theirs), Vec::<String>::new());
}

#[test]
fn results_narrow_to_the_kinds_sources_scale_domains_and_layers_named() {
    let db = fixture();
    let key = db.key();
    let id = layer(&db, &key, &json!({"title": "Near", "visibility": "public"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut item = sn1987a("near");
    item["geometry"] = json!({"frame": "icrs", "unit": "megaparsec", "geometry": {"kind": "point", "at": [101.0, 0.0, 0.0]}});
    publish(&db, &key, &id, &item);
    let with = |filters: Value| {
        page(
            &db,
            &json!({"volume": sphere([100.0, 0.0, 0.0], 50.0), "filters": filters}),
        )
    };
    let everything = page(&db, &json!({"volume": sphere([100.0, 0.0, 0.0], 50.0)}));
    assert_eq!(
        everything,
        [
            "object:1001",
            "object:1002",
            "object:1003",
            &format!("item:{id}/near")
        ]
    );
    assert_eq!(
        with(json!({"kinds": ["galaxy"]})),
        ["object:1001", "object:1002", "object:1003"]
    );
    assert_eq!(with(json!({"kinds": ["star"]})), Vec::<String>::new());
    assert_eq!(
        with(json!({"sources": ["hipparcos-2007"]})),
        ["object:1002"]
    );
    assert_eq!(
        with(json!({"scale_domains": ["stars"]})),
        Vec::<String>::new()
    );
    assert_eq!(
        with(json!({"layers": [id.clone()]})),
        [format!("item:{id}/near")]
    );
    let voids = page(
        &db,
        &json!({"volume": sphere([-100.0, -100.0, 160.0], 30.0), "filters": {"kinds": ["void"]}}),
    );
    assert_eq!(voids, ["structure:bootes-void"]);
}

#[test]
fn a_malformed_volume_or_filter_answers_400_pointing_at_each_field() {
    let db = fixture();
    let cases = [
        (
            json!({"frame": "fk4_b1950", "unit": "megaparsec", "form": "sphere", "centre": [0, 0, 0], "radius": 1}),
            vec!["/volume/frame"],
        ),
        (
            json!({"frame": "icrs", "unit": "furlong", "form": "sphere", "centre": [0, 0, 0], "radius": 0}),
            vec!["/volume/radius", "/volume/unit"],
        ),
        (
            json!({"frame": "icrs", "unit": "megaparsec", "form": "sphere", "centre": [0, 0, 0], "radius": -1}),
            vec!["/volume/radius"],
        ),
        (
            json!({"frame": "icrs", "unit": "degree", "form": "box", "min": [0, 0, 0], "max": [1, 0, 1]}),
            vec!["/volume/max", "/volume/unit"],
        ),
        (
            json!({"frame": "icrs", "unit": "megaparsec", "form": "box", "min": [0, 0, 0], "max": [1, 0, 1]}),
            vec!["/volume/max"],
        ),
        (
            json!({"frame": "icrs", "unit": "megaparsec", "form": "cone", "lon": 0, "lat": 0, "opening_angle": 181, "near": 0, "far": 1}),
            vec!["/volume/opening_angle"],
        ),
        (
            json!({"frame": "galactic", "unit": "parsec", "form": "cone", "lon": 0, "lat": 95, "opening_angle": 10, "near": 3, "far": 2, "colour": "red"}),
            vec!["/volume/colour", "/volume/far", "/volume/lat"],
        ),
        (
            json!({"frame": "icrs", "unit": "parsec", "form": "cylinder"}),
            vec!["/volume/form"],
        ),
    ];
    for (volume, expected) in cases {
        let (response, problem) = query(&db, &json!({"volume": volume}));
        assert_eq!(response.status, 400, "{volume}");
        assert_eq!(problem["type"], "urn:univ:problem:query-invalid");
        assert_eq!(pointers(&problem), expected, "{volume}");
    }
    let filters = json!({"kinds": ["unicorn", "galaxy"], "sources": [], "scale_domains": ["stars", "dark_sector"], "layers": ["lay_none"]});
    let (response, problem) = query(
        &db,
        &json!({"volume": sphere([0.0; 3], 1.0), "filters": filters}),
    );
    assert_eq!(response.status, 400);
    assert_eq!(pointers(&problem), ["/filters/sources"]);
    let filters = json!({"kinds": ["unicorn", "galaxy"], "scale_domains": ["stars", "dark_sector"], "layers": ["lay_none"]});
    let (_, problem) = query(
        &db,
        &json!({"volume": sphere([0.0; 3], 1.0), "filters": filters}),
    );
    assert_eq!(
        pointers(&problem),
        [
            "/filters/kinds/0",
            "/filters/layers/0",
            "/filters/scale_domains/1"
        ]
    );
}

#[test]
fn the_first_page_gives_the_tile_nodes_covering_the_volume_each_fetched_by_its_address() {
    let db = fixture();
    let body = json!({"volume": sphere([100.0, 0.0, 0.0], 5.0), "tiles": {"max_level": 2}});
    let (_, page) = query(&db, &body);
    let tiles = page["tiles"].as_array().unwrap();
    let domains: Vec<&str> = tiles
        .iter()
        .map(|t| t["scale_domain"].as_str().unwrap())
        .collect();
    assert_eq!(domains, ["cosmic_web", "stars"]);
    // The cosmic web's octree: a root of ±168 Mpc, octant bit 0 for high x, 1 for high y, 2
    // for high z. The sphere about x = 100 lies in the four level-1 octants of high x, and in
    // each of them in the level-2 octant of high x nearest the x axis.
    let nodes: Vec<&str> = tiles[0]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert_eq!(nodes, ["r17", "r35", "r53", "r71"]);
    assert_eq!(
        tiles[1],
        json!({"scale_domain": "stars", "level": 1, "nodes": []}),
        "100 Mpc is beyond the stars"
    );
    for node in nodes {
        let tile = db.get(
            &format!("/v1/scale-domains/cosmic_web/tiles/{node}"),
            "release=2",
        );
        assert_eq!(tile.status, 200, "{node}");
    }
    let shallow = json!({"volume": sphere([100.0, 0.0, 0.0], 5.0), "tiles": {"max_level": 1}});
    assert_eq!(
        query(&db, &shallow).1["tiles"][0]["nodes"],
        json!(["r1", "r3", "r5", "r7"])
    );
    let only_stars =
        json!({"volume": sphere([100.0, 0.0, 0.0], 5.0), "filters": {"scale_domains": ["stars"]}});
    assert_eq!(
        query(&db, &only_stars).1["tiles"].as_array().unwrap().len(),
        1
    );
}
