//! Object lookups (look-up-an-object), read from the test kit's release 2 and its id ledger.

use univ_model::api::lookup::LedgerChange;

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

/// What `resolve` gave, shortened to the objects and the (id, release, change) of each step.
fn outcome(resolved: Result<u64, Resolved>) -> (Vec<u64>, Vec<(u64, u32, String)>) {
    let steps = |history: Vec<LedgerChange>| {
        history
            .into_iter()
            .map(|c| (c.id, c.release, c.change))
            .collect()
    };
    match resolved {
        Ok(id) => (vec![id], Vec::new()),
        Err(Resolved::Merged { into, history }) => (vec![into], steps(history)),
        Err(Resolved::Split { into, history }) => (into, steps(history)),
        Err(Resolved::Retired { history }) => (Vec::new(), steps(history)),
        Err(Resolved::Unknown) => (Vec::new(), Vec::new()),
    }
}

fn step(id: u64, release: u32, change: &str) -> (u64, u32, String) {
    (id, release, change.to_owned())
}

#[test]
fn an_id_merged_into_one_object_leads_to_it_naming_the_release_that_merged_it() {
    assert_eq!(
        outcome(resolve(2, 5)),
        (vec![10], vec![step(5, 2, "merged_into")])
    );
}

#[test]
fn an_id_split_into_more_than_one_object_leads_to_each_of_them() {
    assert_eq!(
        outcome(resolve(2, 6)),
        (vec![20, 21], vec![step(6, 2, "split_into")])
    );
}

#[test]
fn an_id_replaced_twice_leads_to_the_object_the_read_release_holds_naming_each_change() {
    assert_eq!(
        outcome(resolve(2, 8)),
        (
            vec![30],
            vec![step(8, 1, "merged_into"), step(9, 2, "merged_into")]
        )
    );
}

#[test]
fn a_merge_into_an_object_later_retired_names_both_changes() {
    let db = db();
    db.exec(
        "INSERT INTO ids_replaced VALUES (60, 1, 'merged_into', '[61]'), (61, 2, 'retired', '[]');",
    );
    let resolved = block_on(replaced(&db, 2, 60)).unwrap();
    assert_eq!(
        outcome(Err(resolved)),
        (
            Vec::new(),
            vec![step(60, 1, "merged_into"), step(61, 2, "retired")]
        )
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
    assert_eq!(
        outcome(resolve(2, 7)),
        (Vec::new(), vec![step(7, 2, "retired")])
    );
    assert_eq!(resolve(2, 999), Err(Resolved::Unknown));
}

#[test]
fn a_split_into_many_objects_lists_every_one() {
    let db = db();
    let many: Vec<String> = (1000..1040).map(|id| format!("(2, {id}, 1, 1)")).collect();
    db.exec(&format!(
        "INSERT INTO objects (release, id, kind, placed) VALUES {};
         INSERT INTO ids_replaced VALUES (70, 2, 'split_into', '[{}]');",
        many.join(", "),
        (1000..1040)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ));
    let Resolved::Split { into, .. } = block_on(replaced(&db, 2, 70)).unwrap() else {
        panic!("a split");
    };
    assert_eq!(into.len(), 40);
}

#[test]
fn a_ledger_loop_ends() {
    let db = db();
    db.exec("INSERT INTO ids_replaced VALUES (80, 1, 'merged_into', '[81]'), (81, 2, 'merged_into', '[80]');");
    assert!(matches!(
        block_on(replaced(&db, 2, 80)).unwrap(),
        Resolved::Retired { .. }
    ));
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
    let (response, document) = db().get_json("/v1/objects/5", "");
    assert_eq!(response.status, 301);
    assert_eq!(
        response.header("location"),
        Some("/v1/objects/10?release=2")
    );
    let merged = document;
    assert_eq!(merged["merged_into"], 10);
    assert_eq!(merged["replaced_in"], 2);
}

#[test]
fn a_split_id_answers_300_with_the_objects_that_replaced_it() {
    let (response, document) = db().get_json("/v1/objects/6", "");
    assert_eq!(response.status, 300);
    let choices = document;
    assert_eq!(choices["ambiguity"], "split");
    assert_eq!(choices["replaced_in"], 2);
    assert_eq!(choices["objects"][0]["href"], "/v1/objects/20?release=2");
    assert_eq!(choices["objects"][1]["id"], 21);
}

#[test]
fn a_name_more_than_one_object_carries_answers_300_with_each() {
    let (response, document) = db().get_json("/v1/objects", "name=twins");
    assert_eq!(response.status, 300);
    let choices = document;
    assert_eq!(choices["ambiguity"], "shared_name");
    assert_eq!(choices["objects"].as_array().unwrap().len(), 2);
    assert_eq!(choices["objects"][0]["names"][0], "Twins");
}

#[test]
fn a_retired_id_answers_404_naming_the_release_that_retired_it() {
    let (response, document) = db().get_json("/v1/objects/7", "");
    assert_eq!(response.status, 404);
    assert_eq!(document["replaced_in"], 2);
    assert_eq!(document["history"][0]["change"], "retired");
}
