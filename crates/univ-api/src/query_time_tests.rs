//! A volume query narrowed by time (filter-by-time): the kind of record compared, intervals
//! that meet, results with no such record, UTC and TT compared on TT, and faulty intervals.

use serde_json::{Value, json};

use crate::layers_fixture::{layer, publish, sn1987a};
use crate::query_fixture::{galaxy, hits, pointers, query, sphere, volume_db};
use crate::testkit::Sqlite;

/// A galaxy and three items at (10, 0, 0) Mpc: SN 1987A (its arrival in UTC, ±60 s, and its
/// source event 168,000 ± 4,000 years before J2000.0), GW170817 (its arrival in TT, none
/// reported), and an item with no time record. The layer's id.
fn fixture() -> (Sqlite, String) {
    let db = volume_db(&[galaxy(1001, [10.0, 0.0, 0.0])]);
    let key = db.key();
    let id = layer(
        &db,
        &key,
        &json!({"title": "Events", "visibility": "public"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let at = json!({"frame": "icrs", "unit": "megaparsec", "geometry": {"kind": "point", "at": [10.0, 0.0, 0.0]}});
    let mut sn = sn1987a("sn1987a");
    sn["geometry"] = at.clone();
    let mut gw = sn1987a("gw170817");
    gw["geometry"] = at.clone();
    gw["time_records"] = json!([{"kind": "arrival", "timescale": "tt", "instant": "2017-08-17T12:42:09.184",
        "uncertainty": {"form": "none_reported"}}]);
    let mut timeless = sn1987a("timeless");
    timeless["geometry"] = at;
    timeless["time_records"] = json!([]);
    for item in [sn, gw, timeless] {
        assert_eq!(publish(&db, &key, &id, &item).0.status, 201);
    }
    (db, id)
}

fn utc(instant: &str) -> Value {
    json!({"timescale": "utc", "instant": instant})
}

fn between(db: &Sqlite, kind: &str, from: Value, to: Value) -> Vec<String> {
    let mut time = json!({"kind": kind});
    time["from"] = from;
    time["to"] = to;
    let body = json!({"volume": sphere([10.0, 0.0, 0.0], 1.0), "time": time});
    let (response, page) = query(db, &body);
    assert_eq!(response.status, 200, "{page}");
    hits(&page)
}

#[test]
fn the_query_names_the_kind_of_time_record_compared() {
    let (db, id) = fixture();
    let day = (utc("1987-02-23T00:00:00Z"), utc("1987-02-24T00:00:00Z"));
    assert_eq!(
        between(&db, "arrival", day.0.clone(), day.1.clone()),
        [format!("item:{id}/sn1987a")]
    );
    assert_eq!(
        between(&db, "discovery", day.0.clone(), day.1.clone()),
        Vec::<String>::new()
    );
    assert_eq!(
        between(&db, "source_event", day.0, day.1),
        Vec::<String>::new()
    );
    let ages = (
        json!({"timescale": "tt", "years_before_j2000": 170_000}),
        json!({"timescale": "tt", "years_before_j2000": 165_000}),
    );
    assert_eq!(
        between(&db, "source_event", ages.0, ages.1),
        [format!("item:{id}/sn1987a")]
    );
}

#[test]
fn a_record_matches_when_its_uncertainty_interval_meets_the_querys() {
    let (db, id) = fixture();
    // The arrival is 07:35:35 UTC ± 60 s: from 07:34:35 to 07:36:35.
    let sn = [format!("item:{id}/sn1987a")];
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("1987-02-23T07:00:00Z"),
            utc("1987-02-23T07:34:50Z")
        ),
        sn
    );
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("1987-02-23T07:36:30Z"),
            utc("1987-02-23T08:00:00Z")
        ),
        sn
    );
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("1987-02-23T07:00:00Z"),
            utc("1987-02-23T07:34:30Z")
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("1987-02-23T07:36:40Z"),
            utc("1987-02-23T08:00:00Z")
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_result_with_no_time_record_of_the_kind_named_does_not_match() {
    let (db, id) = fixture();
    let everything = query(&db, &json!({"volume": sphere([10.0, 0.0, 0.0], 1.0)})).1;
    assert_eq!(
        hits(&everything).len(),
        4,
        "the galaxy and the three items without a time filter"
    );
    let all_time = between(
        &db,
        "arrival",
        json!({"timescale": "tt", "years_before_j2000": 1e9}),
        utc("2027-01-01T00:00:00Z"),
    );
    assert_eq!(
        all_time,
        [format!("item:{id}/gw170817"), format!("item:{id}/sn1987a")]
    );
}

#[test]
fn a_time_in_utc_and_one_in_tt_are_compared_on_tt() {
    let (db, id) = fixture();
    // In August 2017 TT − UTC = 37 leap seconds + 32.184 s: GW170817's arrival at
    // 12:42:09.184 TT is 12:41:00 UTC (IERS Bulletin C).
    let gw = [format!("item:{id}/gw170817")];
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("2017-08-17T12:40:59.5Z"),
            utc("2017-08-17T12:41:00.5Z")
        ),
        gw
    );
    assert_eq!(
        between(
            &db,
            "arrival",
            utc("2017-08-17T12:42:09Z"),
            utc("2017-08-17T12:42:10Z")
        ),
        Vec::<String>::new()
    );
    let tt = |instant: &str| json!({"timescale": "tt", "instant": instant});
    assert_eq!(
        between(
            &db,
            "arrival",
            tt("2017-08-17T12:42:09"),
            tt("2017-08-17T12:42:10")
        ),
        gw
    );
}

#[test]
fn a_faulty_interval_answers_400_pointing_at_the_field() {
    let (db, _) = fixture();
    let cases = [
        (
            json!({"kind": "arrival", "from": utc("2000-01-02T00:00:00Z"), "to": utc("2000-01-01T00:00:00Z")}),
            vec!["/time/to"],
        ),
        (
            json!({"kind": "arrival", "from": {"timescale": "tcb", "instant": "2000-01-01T00:00:00"}, "to": utc("2000-01-01T00:00:00Z")}),
            vec!["/time/from/timescale"],
        ),
        (
            json!({"kind": "arrival", "from": utc("1960-06-01T00:00:00Z"), "to": utc("2000-01-01T00:00:00Z")}),
            vec!["/time/from/instant"],
        ),
        (
            json!({"kind": "arrival", "from": utc("2000-01-01T00:00:00Z"), "to": {"timescale": "tt", "instant": "2001-01-01T00:00:00", "years_before_j2000": 1}}),
            vec!["/time/to/years_before_j2000"],
        ),
        (
            json!({"kind": "afterglow", "from": utc("2000-01-01T00:00:00Z"), "to": {"timescale": "tt"}}),
            vec!["/time/kind", "/time/to"],
        ),
    ];
    for (time, expected) in cases {
        let (response, problem) = query(
            &db,
            &json!({"volume": sphere([10.0, 0.0, 0.0], 1.0), "time": time}),
        );
        assert_eq!(response.status, 400, "{time}");
        assert_eq!(pointers(&problem), expected, "{time}");
    }
}
