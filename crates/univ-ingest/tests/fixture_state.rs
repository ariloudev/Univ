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
use std::process::Command;

use common::workspace;
use release_fixture::{build_next, fixture};

/// The variable that names the state directory to write.
const STATE_VARIABLE: &str = "UNIV_FIXTURE_STATE";

/// Runs `univ-ingest <command> <number>` against the fixture's output and `persist`.
fn univ_ingest(command: &str, number: u32, out: &Path, state: &Path, persist: &Path) {
    let run = Command::new(env!("CARGO_BIN_EXE_univ-ingest"))
        .arg(command)
        .arg(number.to_string())
        .arg("--out")
        .arg(out)
        .arg("--state")
        .arg(state)
        .arg("--persist-to")
        .arg(persist)
        .arg("--config")
        .arg(workspace().join("wrangler.toml"))
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "univ-ingest {command} {number}:\n{log}"
    );
}

#[test]
#[ignore = "writes local D1 and R2 with Wrangler for `cargo xtask integration`"]
fn the_fixture_releases_are_published_and_the_first_pruned() {
    let persist = PathBuf::from(std::env::var(STATE_VARIABLE).unwrap());
    let f = fixture();
    let (out, state) = (f.out.path(), f.state.path());
    build_next(&f);
    univ_ingest("publish", 1, out, state, &persist);
    build_next(&f);
    univ_ingest("publish", 2, out, state, &persist);
    univ_ingest("prune", 1, out, state, &persist);
}
