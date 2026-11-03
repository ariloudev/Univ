//! The local D1 and R2 state that Univ Server's end-to-end tests read: the fixture's release
//! published as release 1, a second build published as release 2, and release 1 pruned, in the
//! `--persist-to` directory that `UNIV_FIXTURE_STATE` names. `cargo xtask integration` runs it;
//! by hand: `UNIV_FIXTURE_STATE=<dir> cargo test -p univ-ingest --test fixture_state --
//! --ignored`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::path::{Path, PathBuf};

use release_fixture::{build_next, fixture, univ_ingest};

/// The variable that names the state directory to write.
const STATE_VARIABLE: &str = "UNIV_FIXTURE_STATE";

/// Runs `univ-ingest <command> <number>` and fails with its log when it fails.
fn run_ok(command: &str, number: u32, out: &Path, state: &Path, persist: &Path) {
    let run = univ_ingest(command, number, out, state, persist);
    let log = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "univ-ingest {command} {number}:\n{log}"
    );
}

#[test]
#[ignore = "writes local D1 and R2 with Wrangler for `cargo xtask integration`"]
fn the_fixture_releases_are_published_and_the_first_pruned() {
    let Ok(persist) = std::env::var(STATE_VARIABLE).map(PathBuf::from) else {
        panic!(
            "{STATE_VARIABLE} names no directory: this writes the fixture state for \
             `cargo xtask integration`, which runs it; it is not a test to run with --ignored"
        );
    };
    let f = fixture();
    let (out, state) = (f.out.path(), f.state.path());
    build_next(&f);
    run_ok("publish", 1, out, state, &persist);
    build_next(&f);
    run_ok("publish", 2, out, state, &persist);
    run_ok("prune", 1, out, state, &persist);
}
