//! A build or a publish the owner interrupts stops at a safe point: no release is left half
//! built or half published, and the next run completes it (F-13, AC-121).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use memory_store::{Clock, MemoryStore, publishing};
use release_fixture::{BUILT_AT, build_next, cross, fixture, tiles};
use univ_ingest::publish::journal::Journal;
use univ_ingest::publish::{FileDone, Latest, Progress, PublishError, StoreOptions, publish_with};
use univ_ingest::release::{BuildError, Inputs, PHASES, RELEASES, build_watched, release_dir};
use univ_ingest::state::State;
use univ_ingest::watch::{STOPPED, Watch};

#[test]
fn a_build_stopped_at_each_safe_point_leaves_nothing_and_the_next_build_completes() {
    let f = fixture();
    let crossed = cross(&f);
    let tiles = tiles(&f, &crossed);
    let inputs = Inputs {
        crossed: &crossed,
        tiles: &tiles,
        definitions: &f.definitions,
        grids: &[],
        cache: f.cache.path(),
        built_at: BUILT_AT.to_owned(),
    };
    let building = f.out.path().join(RELEASES).join(".building-1");
    for phase in 1..=PHASES {
        // The owner presses Ctrl-C as the build tells of its phase.
        let stop = AtomicBool::new(false);
        let steps = AtomicU64::new(0);
        let told = |_: String| {
            if steps.fetch_add(1, Ordering::SeqCst) + 1 == phase {
                stop.store(true, Ordering::SeqCst);
            }
        };
        let watch = Watch::new(&stop, &told);
        let error = build_watched(&inputs, f.state.path(), f.out.path(), watch).unwrap_err();
        assert_eq!(
            error,
            BuildError::Release {
                number: Some(1),
                problem: STOPPED.to_owned()
            },
            "stopped at phase {phase}"
        );
        assert_eq!(
            steps.load(Ordering::SeqCst),
            phase,
            "it stops at the next safe point"
        );
        assert!(!building.exists(), "no half-built directory is left");
        assert!(!release_dir(f.out.path(), 1).exists());
        let state = State::read(f.state.path()).unwrap();
        assert!(state.ledger.releases().is_empty(), "no release is recorded");
    }
    let told = |_: String| {};
    let never = AtomicBool::new(false);
    let watch = Watch::new(&never, &told);
    let built = build_watched(&inputs, f.state.path(), f.out.path(), watch).unwrap();
    assert_eq!(built.number, 1, "the stopped builds used up no number");
    assert!(release_dir(f.out.path(), 1).join("manifest.yaml").is_file());
    let state = State::read(f.state.path()).unwrap();
    assert_eq!(state.ledger.releases().len(), 1);
}

/// An owner who presses Ctrl-C once `after` files are done, keeping each line.
struct Owner {
    after: usize,
    files: usize,
    lines: Vec<String>,
}

impl Progress for Owner {
    fn line(&mut self, line: String) {
        self.lines.push(line);
    }

    fn file(&mut self, file: FileDone<'_>) {
        self.files += 1;
        self.lines.push(file.line);
    }

    fn stopping(&self) -> bool {
        self.files >= self.after
    }
}

#[test]
fn a_publish_stopped_by_the_owner_ends_after_the_unit_in_hand_and_the_next_run_completes_it() {
    let f = fixture();
    let built = build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let now = || clock.now();
    let options = StoreOptions {
        out: f.out.path(),
        state: f.state.path(),
        journals: journals.path(),
        now: &now,
    };
    // The schema, then one R2 file a unit: the owner stops it after the third.
    let mut owner = Owner {
        after: 3,
        files: 0,
        lines: Vec::new(),
    };
    let error = publish_with(&mut store, 1, &options, &mut owner).unwrap_err();
    let PublishError::Interrupted {
        latest,
        done,
        total,
        problem,
        ..
    } = &error
    else {
        panic!("{error}")
    };
    assert_eq!(
        (*latest, *done, *total, problem.as_str()),
        (Latest(None), 3, built.manifest.files.len() + 1, STOPPED)
    );
    let said = error.to_string();
    assert!(
        said.contains("publishing release 1 again continues from there"),
        "{said}"
    );
    assert_eq!(owner.lines.len(), 3, "{:?}", owner.lines);
    // latest is unmoved and the release is not listed as published; the journal lists each
    // unit done.
    let latest = store
        .db
        .query_row("SELECT release FROM latest", [], |r| r.get::<_, i64>(0));
    assert!(latest.is_err(), "latest points to no release");
    assert_eq!(
        store.int("SELECT COUNT(*) FROM releases WHERE published_at IS NOT NULL"),
        0
    );
    let journal = Journal::read(journals.path(), 1).unwrap().unwrap();
    assert_eq!(journal.done.len(), 3);

    let again = publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    assert_eq!((again.journaled, again.uploaded), (3, *total - 3));
    assert_eq!(store.int("SELECT release FROM latest"), 1);
}
