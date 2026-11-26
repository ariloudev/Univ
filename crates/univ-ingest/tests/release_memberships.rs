//! The memberships a source assigns to researched structures, in a build that takes the
//! researched structures and in one that takes none (cross-match-catalogues).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use release_fixture::{Fixture, built, cross, fixture_with, loaded, tiles};
use univ_ingest::source::takes_structures;

/// The fixture with Reid et al. 2019's masers and without the researched structures: neither
/// their source's definition nor their files.
fn without_structures() -> Fixture {
    let mut f = fixture_with(&["reid-2019-masers"]);
    f.definitions
        .retain(|d| d.id.as_str() != "researched-structures");
    f.structures.clear();
    f
}

#[test]
fn a_build_of_the_masers_that_takes_no_researched_structures_completes() {
    let f = without_structures();
    assert!(!takes_structures(&f.definitions));
    let crossed = cross(&f);
    let laid = tiles(&f, &crossed);
    let release = built(&f, &crossed, &laid, &f.definitions, &f.structures);
    assert!(release.is_ok(), "{:?}", release.err());
}

#[test]
fn in_a_build_without_the_structures_each_maser_is_an_object_and_a_member_of_none() {
    let f = without_structures();
    let crossed = cross(&f);
    let laid = tiles(&f, &crossed);
    let release = built(&f, &crossed, &laid, &f.definitions, &f.structures).unwrap();
    let db = loaded(&release.dir, &release.manifest);
    let masers: i64 = db
        .query_row(
            "SELECT COUNT(DISTINCT e.object_id) FROM entries e \
             WHERE e.release = 1 AND e.object_id IS NOT NULL \
             AND e.source = (SELECT n FROM sources WHERE release = 1 AND id = 'reid-2019-masers')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(masers > 0, "the masers are objects of the release");
    let members: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM memberships WHERE release = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(members, 0);
}

#[test]
fn a_build_that_takes_the_structures_refuses_a_membership_naming_none_of_them() {
    let f = fixture_with(&["reid-2019-masers"]);
    assert!(takes_structures(&f.definitions));
    let crossed = cross(&f);
    let laid = tiles(&f, &crossed);
    let without_arms: Vec<_> = f
        .structures
        .iter()
        .filter(|(id, _)| !id.starts_with("reid-2019-"))
        .cloned()
        .collect();
    let message = built(&f, &crossed, &laid, &f.definitions, &without_arms)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("none of the release's structures"),
        "{message}"
    );
    for named in [
        "reid-2019-masers",
        "column Arm",
        "value \"",
        "structure \"reid-2019-",
    ] {
        assert!(message.contains(named), "{named} in {message}");
    }
}
