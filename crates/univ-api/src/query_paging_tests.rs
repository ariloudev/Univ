//! Paging a bounded volume query: cursors, stable order, the cap of 10,000 results, and the
//! layers a cursor records (query-a-bounded-volume), with the items a layer's changes take out
//! of query results (create-and-configure-a-layer, revise-or-retract-an-item).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use crate::http::{Method, Request};
use crate::layers_fixture::{layer, publish, sn1987a};
use crate::query_fixture::{galaxy, hits, pointers, query, request, sphere, volume_db};
use crate::testkit::Sqlite;

/// `n` galaxies from id 3000 in a block about (-500, 5, 5) Mpc, and a public layer with an
/// item there; the layer's id and its key.
fn crowded(n: u64) -> (Sqlite, String, String) {
    let objects: Vec<_> = (0..n)
        .map(|i| {
            let n = |v: u64| f64::from(u32::try_from(v).unwrap());
            let at = [-510.0 + n(i % 20), n(i / 20 % 10), n(i / 200) * 0.1];
            galaxy(3_000 + i, at)
        })
        .collect();
    let db = volume_db(&objects);
    let key = db.key();
    let id = layer(
        &db,
        &key,
        &json!({"title": "Crowd", "visibility": "public"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    publish(&db, &key, &id, &placed("first", [-500.0, 5.0, 0.0]));
    (db, id, key)
}

/// SN 1987A's record as item `item`, moved to `at` Mpc.
fn placed(item: &str, at: [f64; 3]) -> Value {
    let mut item = sn1987a(item);
    item["geometry"] =
        json!({"frame": "icrs", "unit": "megaparsec", "geometry": {"kind": "point", "at": at}});
    item
}

fn crowd() -> Value {
    json!({"volume": {"frame": "icrs", "unit": "megaparsec", "form": "box",
        "min": [-520.0, -1.0, -1.0], "max": [-480.0, 11.0, 60.0]}})
}

/// Every page of `body`, following cursors.
fn pages(db: &Sqlite, body: &Value) -> Vec<Value> {
    let mut all = Vec::new();
    let mut body = body.clone();
    loop {
        let (response, page) = query(db, &body);
        assert_eq!(response.status, 200, "{page}");
        let next = page["next"].as_str().map(str::to_owned);
        all.push(page);
        match next {
            Some(cursor) => body["cursor"] = json!(cursor),
            None => return all,
        }
    }
}

#[test]
fn each_page_carries_a_cursor_to_the_next_while_results_remain() {
    let (db, id, _) = crowded(1_500);
    let all = pages(&db, &crowd());
    assert_eq!(all.len(), 2);
    assert_eq!(all[0]["results"].as_array().unwrap().len(), 1_000);
    assert_eq!(all[0]["served"], 1_000);
    assert_eq!(all[1]["results"].as_array().unwrap().len(), 501);
    assert_eq!(all[1]["served"], 1_501);
    assert!(all[1].get("next").is_none());
    assert_eq!(hits(&all[1]).last().unwrap(), &format!("item:{id}/first"));
}

#[test]
fn paging_a_query_whose_layers_did_not_change_gives_the_same_pages_in_one_order() {
    let (db, _, _) = crowded(1_500);
    let (first, again) = (pages(&db, &crowd()), pages(&db, &crowd()));
    assert_eq!(first, again);
    let ids: Vec<String> = first.iter().flat_map(hits).collect();
    let objects: Vec<u64> = ids
        .iter()
        .filter_map(|h| h.strip_prefix("object:")?.parse().ok())
        .collect();
    assert_eq!(
        objects,
        (3_000..4_500).collect::<Vec<u64>>(),
        "every object once, by id"
    );
}

#[test]
fn a_query_matching_more_than_10000_returns_the_first_10000_and_says_it_was_truncated() {
    let (db, _, _) = crowded(10_050);
    let all = pages(&db, &crowd());
    assert_eq!(all.len(), 10);
    let last = all.last().unwrap();
    assert_eq!(last["served"], 10_000);
    assert_eq!(last["truncated"], true);
    assert!(last.get("next").is_none());
    assert!(all[..9].iter().all(|p| p["truncated"] == false));
    let ids: Vec<String> = all.iter().flat_map(hits).collect();
    assert_eq!(ids.len(), 10_000);
    assert_eq!(ids.last().unwrap(), "object:12999");
}

#[test]
fn a_cursor_records_the_release_read_and_each_layers_change_sequence() {
    let (db, id, _) = crowded(1_200);
    let (_, page) = query(&db, &crowd());
    let cursor: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(page["next"].as_str().unwrap().split_once('.').unwrap().0)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(cursor["release"], 2);
    assert_eq!(cursor["layers"], json!({id: 1}));
    assert_eq!(cursor["after"], json!({"part": "object", "id": 3_999}));
}

#[test]
fn a_layer_changed_between_pages_answers_409_naming_it_with_its_current_sequence() {
    let (db, id, key) = crowded(1_200);
    let (_, page) = query(&db, &crowd());
    let mut next = crowd();
    next["cursor"] = page["next"].clone();
    publish(&db, &key, &id, &placed("second", [-499.0, 5.0, 0.0]));
    let (response, problem) = query(&db, &next);
    assert_eq!(response.status, 409, "{problem}");
    assert_eq!(problem["type"], "urn:univ:problem:query-layers-changed");
    assert_eq!(problem["layers"], json!([{"layer": id, "sequence": 2}]));
    let patch = Request::new(Method::Patch, &format!("/v1/layers/{id}"))
        .with_json(&json!({"visibility": "private"}));
    db.with_key(&key, patch);
    let (_, problem) = query(&db, &next);
    assert_eq!(problem["layers"], json!([{"layer": id, "readable": false}]));
    let (_, problem) = db.with_key(&key, request(&next));
    assert_eq!(
        problem["layers"],
        json!([{"layer": id, "sequence": 3}]),
        "its owner still reads it"
    );
}

#[test]
fn no_page_skips_or_repeats_a_result_because_a_layer_changed() {
    let (db, id, key) = crowded(1_200);
    let (_, page) = query(&db, &crowd());
    let mut next = crowd();
    next["cursor"] = page["next"].clone();
    // An item that sorts before the cursor's position would be skipped, one after it served
    // twice if the layer's change were not noticed: the next page is refused instead.
    publish(&db, &key, &id, &placed("another", [-500.0, 6.0, 0.0]));
    assert_eq!(query(&db, &next).0.status, 409);
    let all = pages(&db, &crowd());
    let items: Vec<String> = all
        .iter()
        .flat_map(hits)
        .filter(|h| h.starts_with("item:"))
        .collect();
    assert_eq!(
        items,
        [format!("item:{id}/another"), format!("item:{id}/first")]
    );
}

#[test]
fn a_cursor_of_another_query_or_one_no_page_gave_answers_400_and_a_later_page_keeps_its_release() {
    let (db, _, _) = crowded(1_200);
    let (_, page) = query(&db, &crowd());
    let mut other = json!({"volume": sphere([0.0; 3], 1.0)});
    other["cursor"] = page["next"].clone();
    let (response, problem) = query(&db, &other);
    assert_eq!(response.status, 400);
    assert_eq!(pointers(&problem), ["/cursor"]);
    other["cursor"] = json!("not-a-cursor");
    assert_eq!(pointers(&query(&db, &other).1), ["/cursor"]);
    db.exec("UPDATE releases SET published_at = '2026-11-03T09:00:00Z' WHERE number = 3; UPDATE latest SET release = 3;");
    let mut next = crowd();
    next["cursor"] = page["next"].clone();
    let (response, later) = query(&db, &next);
    assert_eq!(response.header("univ-release"), Some("2"));
    assert_eq!(later["release"], 2);
    let mut pinned_elsewhere = request(&next);
    pinned_elsewhere.query = "release=3".to_owned();
    assert_eq!(db.answer(&pinned_elsewhere).status, 400);
}

#[test]
fn a_layer_made_private_and_a_retracted_item_leave_query_results() {
    let (db, id, key) = crowded(0);
    let near = json!({"volume": sphere([-500.0, 5.0, 0.0], 1.0)});
    assert_eq!(hits(&query(&db, &near).1), [format!("item:{id}/first")]);
    let retraction = json!({"replaces": 1, "reason": "a test"});
    let retract = format!("/v1/layers/{id}/items/first/retraction");
    assert_eq!(
        db.with_key(
            &key,
            Request::new(Method::Post, &retract).with_json(&retraction)
        )
        .0
        .status,
        201
    );
    assert_eq!(hits(&query(&db, &near).1), Vec::<String>::new());
    publish(&db, &key, &id, &placed("second", [-500.0, 5.0, 0.5]));
    assert_eq!(hits(&query(&db, &near).1), [format!("item:{id}/second")]);
    let patch = Request::new(Method::Patch, &format!("/v1/layers/{id}"))
        .with_json(&json!({"visibility": "private"}));
    db.with_key(&key, patch);
    assert_eq!(hits(&query(&db, &near).1), Vec::<String>::new());
    let (_, owners) = db.with_key(&key, request(&near));
    assert_eq!(hits(&owners), [format!("item:{id}/second")]);
}

#[test]
fn a_cursor_changed_by_its_client_is_refused() {
    let (db, _, _) = crowded(1_200);
    let (_, page) = query(&db, &crowd());
    let (json_part, signature) = page["next"].as_str().unwrap().split_once('.').unwrap();
    let mut cursor: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(json_part).unwrap()).unwrap();
    cursor["served"] = json!(0);
    let forged = format!("{}.{signature}", URL_SAFE_NO_PAD.encode(cursor.to_string()));
    let mut next = crowd();
    next["cursor"] = json!(forged);
    let (response, problem) = query(&db, &next);
    assert_eq!(response.status, 400, "a cursor reset to serve 10,000 more");
    assert_eq!(pointers(&problem), ["/cursor"]);
}

#[test]
fn a_later_page_may_leave_out_the_tiles_the_first_asked_for() {
    let (db, _, _) = crowded(1_200);
    let mut first = crowd();
    first["tiles"] = json!({"max_level": 1});
    let (_, page) = query(&db, &first);
    assert!(page["tiles"].is_array());
    let mut next = crowd();
    next["cursor"] = page["next"].clone();
    let (response, later) = query(&db, &next);
    assert_eq!(response.status, 200, "{later}");
    assert!(
        later.get("tiles").is_none(),
        "tiles come on the first page alone"
    );
}
