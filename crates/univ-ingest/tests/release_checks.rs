//! The checks a release passes before it is complete (AC-86 to AC-89), each failing on the
//! fixture of `release_fixture` with one thing broken: the build names the check and its cause,
//! and uses up no release number (AC-90).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::path::Path;

use common::normalized_fixture;
use memory_store::{Clock, MemoryStore, publishing};
use release_fixture::{Fixture, build_next, built, cross, fixture, record, tiles};
use univ_ingest::crossmatch::CrossMatched;
use univ_ingest::publish::PublishError;
use univ_ingest::release::{
    BuildError, D1_FINITE, D1_KEYS, ENTRY_COUNTS, REDISTRIBUTION, STRUCTURE_CITATIONS,
    TILE_OBJECTS, list, release_dir,
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

#[test]
fn a_number_that_is_not_finite_fails_the_build_naming_its_table_row_and_column() {
    fails_and_uses_no_number(
        D1_FINITE,
        "table measurements, row (release = 1, object_id = ",
        |crossed, _, _, _| {
            let distance = crossed
                .objects
                .iter_mut()
                .flat_map(|o| o.object.entries.iter_mut())
                .find_map(|e| e.entry.distances.first_mut())
                .unwrap();
            distance.normalized.value = f64::NAN;
        },
    );
    let f = fixture();
    let mut crossed = cross(&f);
    let tiles = tiles(&f, &crossed);
    let entry = crossed
        .objects
        .iter_mut()
        .flat_map(|o| o.object.entries.iter_mut())
        .find(|e| !e.entry.distances.is_empty())
        .unwrap();
    entry.entry.distances[0].normalized.value = f64::INFINITY;
    let error = built(&f, &crossed, &tiles, &f.definitions, &f.structures).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(", column value: inf is not a finite number"),
        "{error}"
    );
}

#[test]
fn two_rows_with_one_primary_key_fail_the_build() {
    fails_and_uses_no_number(
        D1_KEYS,
        "table objects: 1 row(s) repeat a primary key another row has, from (release = 1, id = ",
        |crossed, _, _, _| {
            let twin = crossed.objects[0].clone();
            crossed.objects.push(twin);
        },
    );
}

#[test]
fn a_failure_while_recording_the_state_uses_no_number_and_leaves_no_release_publish_accepts() {
    let f = fixture();
    let crossed = cross(&f);
    let laid = tiles(&f, &crossed);
    // `current` cannot be switched: its part file's place is taken by a directory.
    let blocked = f.state.path().join("current.part");
    std::fs::create_dir_all(blocked.join("in-the-way")).unwrap();
    let error = built(&f, &crossed, &laid, &f.definitions, &f.structures).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("number is still free") && message.contains("cannot be recorded"),
        "{message}"
    );
    assert!(!release_dir(f.out.path(), 1).exists());
    assert_eq!(
        State::read(f.state.path()).unwrap().ledger.releases().len(),
        0
    );
    assert!(!f.state.path().join("states/1").exists());
    std::fs::remove_dir_all(&blocked).unwrap();
    let kept = Fixture {
        state: f.state,
        out: f.out,
        ..fixture()
    };
    let again = build_next(&kept);
    assert_eq!(again.number, 1);

    // A build stopped between its release directory's rename and the state's switch leaves a
    // release the state does not record: publish refuses it, and the next build names it.
    let stopped = fixture();
    let dir = release_dir(stopped.out.path(), 1);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    copy_dir(&again.dir, &dir);
    let journals = tempfile::tempdir().unwrap();
    let mut store = MemoryStore::default();
    let refused = publishing(
        &mut store,
        1,
        stopped.out.path(),
        stopped.state.path(),
        journals.path(),
        &Clock::new(),
    )
    .unwrap_err();
    assert!(
        matches!(&refused, PublishError::NotBuilt { problem, .. }
            if problem.contains("does not record it")),
        "{refused}"
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
    let crossed = cross(&stopped);
    let tiles = tiles(&stopped, &crossed);
    let error = built(
        &stopped,
        &crossed,
        &tiles,
        &stopped.definitions,
        &stopped.structures,
    )
    .unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("already exists") && message.contains("Nothing is repaired"),
        "{message}"
    );
    assert!(dir.join("manifest.yaml").exists(), "nothing was removed");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}
