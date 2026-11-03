//! The checks a release passes before it is complete (AC-86 to AC-89), each failing on the
//! fixture of `release_fixture` with one thing broken: the build names the check and its cause,
//! and uses up no release number (AC-90).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::path::Path;

use common::normalized_fixture;
use release_fixture::{Fixture, build_next, built, cross, fixture, record, tiles};
use univ_ingest::crossmatch::CrossMatched;
use univ_ingest::release::{
    BuildError, ENTRY_COUNTS, REDISTRIBUTION, STRUCTURE_CITATIONS, TILE_OBJECTS, list, release_dir,
};
use univ_ingest::source::{RedistributionStatus, SourceDefinition};
use univ_ingest::state::{State, pending};
use univ_ingest::structures::Structure;

/// Builds with `break_it` applied, expects it to fail naming `check` with `cause` in it, and
/// checks that no number was used up: the next good build is release 1.
fn fails_and_uses_no_number(
    check: &str,
    cause: &str,
    break_it: impl FnOnce(
        &mut CrossMatched,
        &mut Vec<SourceDefinition>,
        &mut Vec<(String, Structure)>,
        &Path,
    ),
) {
    let f = fixture();
    let mut crossed = cross(&f);
    let tiles = tiles(&f, &crossed);
    let (mut definitions, mut structures) = (f.definitions.clone(), f.structures.clone());
    break_it(
        &mut crossed,
        &mut definitions,
        &mut structures,
        f.cache.path(),
    );
    let error = built(&f, &crossed, &tiles, &definitions, &structures).unwrap_err();
    let BuildError::Checks { number, failed } = &error else {
        unreachable!("{error}")
    };
    assert_eq!(*number, 1);
    let named = failed
        .iter()
        .find(|c| c.check == check)
        .unwrap_or_else(|| unreachable!("{error}"));
    assert!(named.cause.contains(cause), "{}", named.cause);
    let message = error.to_string();
    assert!(
        message.contains(check) && message.contains("number is still free"),
        "{message}"
    );

    assert_eq!(
        State::read(f.state.path()).unwrap().ledger.releases().len(),
        0
    );
    assert!(
        pending(f.state.path()).exists(),
        "the cross-match's ids stay pending"
    );
    assert_eq!(
        list(f.out.path(), f.out.path()).unwrap(),
        Vec::<String>::new()
    );
    // Mended, the build takes the number the failed one did not use.
    let fixed = fixture();
    let again = build_next(&Fixture {
        state: f.state,
        out: f.out,
        ..fixed
    });
    assert_eq!(again.number, 1);
}

#[test]
fn a_tile_holding_an_object_the_release_lacks_fails_the_build() {
    fails_and_uses_no_number(TILE_OBJECTS, "has no object for", |crossed, _, _, _| {
        let at = crossed
            .objects
            .iter()
            .position(|o| o.position.is_some())
            .unwrap();
        crossed.objects.remove(at);
    });
}

#[test]
fn a_structure_number_citing_no_paper_fails_the_build() {
    fails_and_uses_no_number(
        STRUCTURE_CITATIONS,
        "structure local-void",
        |_, _, structures, _| {
            let (_, void) = structures
                .iter_mut()
                .find(|(id, _)| id == "local-void")
                .unwrap();
            let value = void.values.values_mut().next().unwrap();
            value.cite.paper = "no-such-paper".to_owned();
        },
    );
}

#[test]
fn entry_counts_that_differ_from_the_provenance_fail_the_build() {
    fails_and_uses_no_number(
        ENTRY_COUNTS,
        "twomrs: the release holds",
        |_, _, _, cache| {
            let mut report = normalized_fixture("twomrs").report;
            report.published += 1;
            record(cache, "twomrs", &report);
        },
    );
}

#[test]
fn a_source_that_forbids_redistribution_fails_the_build() {
    fails_and_uses_no_number(
        REDISTRIBUTION,
        "source twomrs forbids",
        |_, definitions, _, _| {
            let twomrs = definitions
                .iter_mut()
                .find(|d| d.id.as_str() == "twomrs")
                .unwrap();
            twomrs.redistribution.status = RedistributionStatus::Forbidden;
        },
    );
}

#[test]
fn a_release_directory_already_there_is_never_overwritten() {
    let f = fixture();
    let crossed = cross(&f);
    let tiles = tiles(&f, &crossed);
    let dir = release_dir(f.out.path(), 1);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("keep"), b"kept").unwrap();
    let error = built(&f, &crossed, &tiles, &f.definitions, &f.structures).unwrap_err();
    assert!(error.to_string().contains("already exists"), "{error}");
    assert_eq!(std::fs::read(dir.join("keep")).unwrap(), b"kept");
    assert_eq!(
        State::read(f.state.path()).unwrap().ledger.releases().len(),
        0
    );
}
