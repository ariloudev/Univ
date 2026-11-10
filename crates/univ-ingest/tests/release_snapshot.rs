//! A snapshot of the fixture's first two releases, to show that a change to how Univ Ingest
//! builds a release leaves what it builds as it was: every manifest (the fixture pins its build
//! time), with each file's SHA-256 and each database's D1 fingerprints, as YAML in the file
//! `UNIV_RELEASE_SNAPSHOT` names. Run it before and after the change and compare the two files:
//! `UNIV_RELEASE_SNAPSHOT=<file> cargo test -p univ-ingest --test release_snapshot -- --ignored`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::fmt::Write;
use std::path::PathBuf;

use release_fixture::{build_next, fixture};

/// The variable that names the file to write.
const SNAPSHOT_VARIABLE: &str = "UNIV_RELEASE_SNAPSHOT";

#[test]
#[ignore = "writes a snapshot to compare by hand; see the module's documentation"]
fn the_fixture_releases_are_written_as_a_snapshot() {
    let Ok(file) = std::env::var(SNAPSHOT_VARIABLE).map(PathBuf::from) else {
        panic!("{SNAPSHOT_VARIABLE} names no file to write the snapshot to");
    };
    let f = fixture();
    let mut snapshot = String::new();
    for _ in 0..2 {
        let manifest = build_next(&f).manifest;
        writeln!(snapshot, "---").unwrap();
        snapshot.push_str(&serde_norway::to_string(&manifest).unwrap());
    }
    std::fs::write(&file, snapshot).unwrap();
}
