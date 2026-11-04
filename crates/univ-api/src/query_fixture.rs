//! A release to query volumes of: release 2 of the layers fixture with objects placed at chosen
//! positions, their cells as univ-model's cell index computes them, and the structures' cells.
//! The positions are a TEST FIXTURE, not data.

use std::fmt::Write;

use serde_json::{Value, json};
use univ_model::geometry::Aabb;
use univ_model::release::cells::{CellSpace, structure_cells};

use crate::http::{Method, Request, Response};
use crate::layers_fixture::db;
use crate::testkit::{Sqlite, body};

/// The terms the fixture's objects are coded in.
pub const GALAXY: i64 = 100;
pub const STAR: i64 = 101;
pub const COSMIC_WEB: i64 = 102;
pub const STARS: i64 = 103;

/// An object to place: its id, kind, domain and position in its domain's unit.
pub struct Placed {
    pub id: u64,
    pub kind: i64,
    pub domain: i64,
    pub at: [f64; 3],
}

pub fn galaxy(id: u64, at: [f64; 3]) -> Placed {
    Placed {
        id,
        kind: GALAXY,
        domain: COSMIC_WEB,
        at,
    }
}

/// The release with `objects` placed, the structures' cells, two structures that the release
/// holds no placed shape of (one with a bounding box alone, one with a member list) and an
/// entry from hipparcos-2007 for each star and for galaxy 1002.
pub fn volume_db(objects: &[Placed]) -> Sqlite {
    let db = db();
    db.exec(&format!(
        "INSERT INTO terms VALUES (2, {GALAXY}, 'galaxy'), (2, {STAR}, 'star'), \
         (2, {COSMIC_WEB}, 'cosmic_web'), (2, {STARS}, 'stars');
         INSERT INTO structures VALUES (2, 'box-only', 'Box Only', 'wall', 'observed', 'cosmic_web',
           205.0, 205.0, 205.0, 'primitive', '{{}}', NULL,
           '{{\"min\":[200.0,200.0,200.0],\"max\":[210.0,210.0,210.0]}}');
         INSERT INTO structures VALUES (2, 'members-only', 'Members Only', 'ring', 'observed',
           'cosmic_web', NULL, NULL, NULL, 'members', '{{}}',
           '{{\"shape\":{{\"frame\":\"icrs\",\"unit\":\"megaparsec\",\"geometry\":{{\"kind\":\"members\",
             \"members\":[{{\"member\":\"tracer\",\"kind\":\"quasar\",\"at\":[300.0,0.0,0.0]}},
                          {{\"member\":\"tracer\",\"kind\":\"quasar\",\"at\":[320.0,0.0,0.0]}}]}}}}}}',
           '{{\"min\":[300.0,0.0,0.0],\"max\":[320.0,0.0,0.0]}}');"
    ));
    let mut sql = String::new();
    for object in objects {
        let space = if object.domain == STARS {
            CellSpace::Stars
        } else {
            CellSpace::CosmicWeb
        };
        let cell = space.cell(object.at).unwrap();
        let [x, y, z] = object.at;
        writeln!(
            sql,
            "INSERT INTO objects (release, id, kind, placed, scale_domain, x, y, z) \
             VALUES (2, {}, {}, 1, {}, {x:?}, {y:?}, {z:?});
             INSERT INTO object_cells VALUES (2, {}, {}, {}, {}, {});",
            object.id,
            object.kind,
            object.domain,
            object.domain,
            cell.ix,
            cell.iy,
            cell.iz,
            object.id
        )
        .unwrap();
        if object.kind == STAR || object.id == 1002 {
            writeln!(
                sql,
                "INSERT INTO entries (release, object_id, entry, source, identifier, resolution) \
                 VALUES (2, {}, 0, 0, 'HIP {}', 0);",
                object.id, object.id
            )
            .unwrap();
        }
    }
    let rows: Vec<(String, String)> = db
        .connection()
        .prepare("SELECT id, bounds FROM structures WHERE release = 2 AND bounds IS NOT NULL")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (id, bounds) in rows {
        let bounds: Aabb = serde_json::from_str(&bounds).unwrap();
        let (level, range) = structure_cells(&bounds).unwrap();
        for cell in range.cells() {
            writeln!(
                sql,
                "INSERT INTO structure_cells VALUES (2, {level}, {}, {}, {}, '{id}');",
                cell.ix, cell.iy, cell.iz
            )
            .unwrap();
        }
    }
    db.exec(&sql);
    db.put(
        "releases/2/tiles/cosmic_web/manifest.json",
        manifest(168.0, 2),
    );
    db.put("releases/2/tiles/stars/manifest.json", manifest(48.0, 1));
    db
}

/// A manifest of a full octree `depth` levels deep below a root from `-radius` to `radius`.
pub fn manifest(radius: f64, depth: u8) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut level = vec![("r".to_owned(), [-radius; 3], [radius; 3])];
    for l in 0..=depth {
        let mut below = Vec::new();
        for (address, min, max) in level {
            nodes.push(
                json!({"address": address, "level": l, "bounds": {"min": min, "max": max},
                "bytes": 1, "sha256": format!("{l}{address}")}),
            );
            if l < depth {
                for octant in 0..8_u8 {
                    let half: [f64; 3] = std::array::from_fn(|i| (max[i] - min[i]) / 2.0);
                    let low: [f64; 3] = std::array::from_fn(|i| {
                        min[i] + if octant >> i & 1 == 1 { half[i] } else { 0.0 }
                    });
                    let high: [f64; 3] = std::array::from_fn(|i| low[i] + half[i]);
                    below.push((format!("{address}{octant}"), low, high));
                }
            }
        }
        level = below;
    }
    serde_json::to_vec(&json!({"encoding": {"version": 1}, "nodes": nodes})).unwrap()
}

/// A volume query's request.
pub fn request(query: &Value) -> Request {
    Request::new(Method::Post, "/v1/query/volume").with_json(query)
}

/// A volume query, anonymously, and its answer.
pub fn query(db: &Sqlite, query: &Value) -> (Response, Value) {
    let response = db.answer(&request(query));
    let json = body(&response);
    (response, json)
}

/// A sphere in the ICRS, in megaparsecs.
pub fn sphere(centre: [f64; 3], radius: f64) -> Value {
    json!({"frame": "icrs", "unit": "megaparsec", "form": "sphere", "centre": centre, "radius": radius})
}

/// Each result as `type:id`, an item's id as `layer/item`.
pub fn hits(page: &Value) -> Vec<String> {
    page["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let id = match r["type"].as_str().unwrap() {
                "item" => format!(
                    "{}/{}",
                    r["layer"].as_str().unwrap(),
                    r["item"].as_str().unwrap()
                ),
                _ => r["id"].to_string().trim_matches('"').to_owned(),
            };
            format!("{}:{id}", r["type"].as_str().unwrap())
        })
        .collect()
}

/// The JSON pointers a refusal names, sorted.
pub fn pointers(problem: &Value) -> Vec<String> {
    let mut list: Vec<String> = problem["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["pointer"].as_str().unwrap().to_owned())
        .collect();
    list.sort();
    list
}
