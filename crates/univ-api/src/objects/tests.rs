//! Object lookups (look-up-an-object), read from the test kit's release 2 and its id ledger.

use super::resolve::{Resolved, by_designation, by_name, present, replaced};
use crate::testkit::{OBJECTS, RELEASES, SOURCES, Sqlite, block_on};

fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES).exec(OBJECTS);
    db
}

/// The id itself when the release holds it, or what it leads to.
fn resolve(release: u32, id: u64) -> Result<u64, Resolved> {
    let db = db();
    if block_on(present(&db, release, id)).unwrap() {
        Ok(id)
    } else {
        Err(block_on(replaced(&db, release, id)).unwrap())
    }
}

#[test]
fn an_id_merged_into_one_object_leads_to_it_naming_the_release_that_merged_it() {
    assert_eq!(
        resolve(2, 5),
        Err(Resolved::Merged {
            into: 10,
            replaced_in: 2
        })
    );
}

#[test]
fn an_id_split_into_more_than_one_object_leads_to_each_of_them() {
    assert_eq!(
        resolve(2, 6),
        Err(Resolved::Split {
            into: vec![20, 21],
            replaced_in: 2
        })
    );
}

#[test]
fn an_id_replaced_twice_leads_to_the_object_the_read_release_holds() {
    assert_eq!(
        resolve(2, 8),
        Err(Resolved::Merged {
            into: 30,
            replaced_in: 1
        })
    );
}

#[test]
fn a_read_of_a_release_that_still_holds_the_id_returns_it_whatever_later_releases_did() {
    assert_eq!(resolve(2, 50), Ok(50));
    assert_eq!(resolve(1, 5), Ok(5));
    assert_eq!(
        resolve(2, 40),
        Err(Resolved::Unknown),
        "release 3's change is not release 2's"
    );
}

#[test]
fn a_retired_id_and_an_id_never_issued_lead_nowhere() {
    assert_eq!(resolve(2, 7), Err(Resolved::Retired { replaced_in: 2 }));
    assert_eq!(resolve(2, 999), Err(Resolved::Unknown));
}

#[test]
fn a_name_resolves_ignoring_letter_case_and_white_space() {
    let db = db();
    for name in ["M31", "m 31", " m31 ", "M\t31", "ANDROMEDA"] {
        let found = block_on(by_name(&db, 2, name)).unwrap();
        assert_eq!(
            found.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [10],
            "{name}"
        );
    }
    let spellings = &block_on(by_name(&db, 2, "m31")).unwrap()[0].1;
    assert_eq!(spellings, &["M 31", "M31"]);
}

#[test]
fn a_name_more_than_one_object_carries_leads_to_each_of_them() {
    let found = block_on(by_name(&db(), 2, "Twins")).unwrap();
    assert_eq!(
        found.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [11, 12]
    );
}

#[test]
fn a_designation_is_a_source_and_its_identifier() {
    let db = db();
    assert_eq!(
        block_on(by_designation(&db, 2, "hipparcos-2007", "HIP 1")).unwrap(),
        [10]
    );
    assert_eq!(
        block_on(by_designation(&db, 2, "hipparcos-2007", "HIP 2")).unwrap(),
        [11, 12]
    );
    let none: Vec<u64> = block_on(by_designation(&db, 2, "twomrs", "HIP 1")).unwrap();
    assert_eq!(none, Vec::<u64>::new());
}

#[test]
fn a_merged_id_answers_301_to_its_object_in_the_same_release() {
    let response = db().get("/v1/objects/5", "");
    assert_eq!(response.status, 301);
    assert_eq!(
        response.header("location"),
        Some("/v1/objects/10?release=2")
    );
    let merged = crate::testkit::body(&response);
    assert_eq!(merged["merged_into"], 10);
    assert_eq!(merged["replaced_in"], 2);
}

#[test]
fn a_split_id_answers_300_with_the_objects_that_replaced_it() {
    let response = db().get("/v1/objects/6", "");
    assert_eq!(response.status, 300);
    let choices = crate::testkit::body(&response);
    assert_eq!(choices["ambiguity"], "split");
    assert_eq!(choices["replaced_in"], 2);
    assert_eq!(choices["objects"][0]["href"], "/v1/objects/20?release=2");
    assert_eq!(choices["objects"][1]["id"], 21);
}

#[test]
fn a_name_more_than_one_object_carries_answers_300_with_each() {
    let response = db().get("/v1/objects", "name=twins");
    assert_eq!(response.status, 300);
    let choices = crate::testkit::body(&response);
    assert_eq!(choices["ambiguity"], "shared_name");
    assert_eq!(choices["objects"].as_array().unwrap().len(), 2);
    assert_eq!(choices["objects"][0]["names"][0], "Twins");
}

#[test]
fn a_retired_id_answers_404_naming_the_release_that_retired_it() {
    let response = db().get("/v1/objects/7", "");
    assert_eq!(response.status, 404);
    assert_eq!(crate::testkit::body(&response)["retired_in"], 2);
}
