//! What the layer tests share: a release database whose ledger issued objects 1 to 60 and
//! holds the test kit's structures, and an item to publish.

use serde_json::{Value, json};

use crate::http::{Method, Request};
use crate::testkit::{OBJECTS, RELEASES, SOURCES, STRUCTURES, Sqlite};

/// The test kit's releases with their objects, structures and the ids each issued.
pub fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES)
        .exec(SOURCES)
        .exec(STRUCTURES)
        .exec(OBJECTS)
        .exec("INSERT INTO ids_issued VALUES (1, 1, 9), (2, 10, 60);");
    db
}

/// A layer `key` creates as `json` asks, as created.
pub fn layer(db: &Sqlite, key: &str, json: &Value) -> Value {
    let (response, layer) = db.with_key(
        key,
        Request::new(Method::Post, "/v1/layers").with_json(json),
    );
    assert_eq!(response.status, 201, "{layer}");
    layer
}

/// An item to publish: SN 1987A, a TEST FIXTURE whose values are rounded from the literature
/// (arrival of the neutrino burst at Kamiokande-II, Hirata et al. 1987; the LMC at about
/// 51.4 kpc, Panagia 1999), linked to the test kit's object 10 as its host.
pub fn sn1987a(id: &str) -> Value {
    json!({
        "id": id,
        "title": "SN 1987A",
        "summary": "A core-collapse supernova in the Large Magellanic Cloud.",
        "geometry": {"frame": "icrs", "unit": "kiloparsec",
                     "geometry": {"kind": "point", "at": [1.977, 18.10, -48.07]}},
        "time_records": [
            {"kind": "arrival", "timescale": "utc", "instant": "1987-02-23T07:35:35Z",
             "uncertainty": {"form": "symmetric", "sigma": 60, "unit": "s"}},
            {"kind": "source_event", "timescale": "tt", "years_before_j2000": 168_000,
             "uncertainty": {"form": "interval", "minus": 4_000, "plus": 4_000, "unit": "a"}}
        ],
        "kind_of_value": "measured",
        "links": [{"relation": "host", "object": 10}],
        "sources": [{"title": "Supernova 1987A", "bibcode": "1989ARA&A..27..629A"}]
    })
}

/// `item` with its `id` member removed: the content of a revision.
pub fn content(item: &Value) -> Value {
    let mut content = item.clone();
    content.as_object_mut().unwrap().remove("id");
    content
}

/// Publishes `item` to layer `layer` with `key`.
pub fn publish(db: &Sqlite, key: &str, layer: &str, item: &Value) -> (crate::Response, Value) {
    db.with_key(
        key,
        Request::new(Method::Post, &format!("/v1/layers/{layer}/items")).with_json(item),
    )
}

/// The rows of `table` in the univ-layers database.
pub fn count(db: &Sqlite, table: &str) -> u64 {
    db.layers
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
