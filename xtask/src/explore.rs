//! Univ Explorer's tasks: its gate in `cargo xtask check`, and `cargo xtask explore`, which runs
//! Univ Server on the real local state with the explorer's dev server beside it.
//!
//! The dev server passes the explorer's reads on to the Worker, adding an extended-tier key
//! issued for the run, so that the explorer reads its tiles at the extended rate and the key
//! never reaches the browser.

use std::io::ErrorKind;
use std::path::Path;

use crate::integration::{REAL_STATE, issue_key, migrate, release_tables, start_ready};
use crate::{Outcome, command};

/// The explorer's directory, relative to the workspace.
pub(crate) const DIR: &str = "clients/explorer";

/// Where the dev server listens, as its `vite.config.ts` fixes it.
const URL: &str = "http://127.0.0.1:5317/";

/// Runs `pnpm` with `args` in the explorer's directory; none when pnpm is not on the path.
fn pnpm(root: &Path, args: &[&str]) -> Option<bool> {
    match command(&root.join(DIR), "pnpm").args(args).status() {
        Ok(status) => Some(status.success()),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("could not run pnpm: {error}");
            Some(false)
        }
    }
}

/// The explorer's gate: its locked dependencies installed, then its own check (typecheck, unit
/// tests with coverage, fallow, build). Skipped when pnpm is not on the path.
pub fn gate(root: &Path) -> Outcome {
    match pnpm(root, &["install", "--frozen-lockfile", "--silent"]) {
        None => {
            eprintln!("skipped: pnpm is not on the path, so {DIR}'s check did not run");
            return Outcome::Skipped;
        }
        Some(false) => return Outcome::Failed,
        Some(true) => {}
    }
    if pnpm(root, &["check"]) == Some(true) {
        Outcome::Passed
    } else {
        Outcome::Failed
    }
}

/// Runs Univ Server on the real local state and the explorer beside it, until interrupted.
pub fn explore(root: &Path) -> bool {
    let state = root.join(REAL_STATE);
    if !state.is_dir() {
        eprintln!(
            "{} holds no published release: run `univ-ingest publish` first",
            state.display()
        );
        return false;
    }
    if pnpm(root, &["install", "--frozen-lockfile", "--silent"]) != Some(true) {
        eprintln!("could not install the explorer's dependencies: is pnpm on the path?");
        return false;
    }
    if !(release_tables(root, &state) && migrate(root, &state)) {
        return false;
    }
    let log = root.join("target/explore/wrangler.log");
    if let Some(dir) = log.parent()
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        eprintln!("could not create {}: {error}", dir.display());
        return false;
    }
    eprintln!(
        "==> Univ Server on the real local state (a first build of the Worker takes minutes)"
    );
    let Some((wrangler, port)) = start_ready(root, &state, &log) else {
        eprintln!("Univ Server did not start; its log is {}", log.display());
        return false;
    };
    let key = match issue_key(port, "extended") {
        Ok(key) => key,
        Err(error) => {
            eprintln!("could not issue the explorer's key: {error}");
            return false;
        }
    };
    eprintln!("==> Univ Explorer at {URL} (Ctrl-C stops both)");
    let served = command(&root.join(DIR), "pnpm")
        .arg("dev")
        .env("UNIV_SERVER", format!("http://127.0.0.1:{port}"))
        .env("UNIV_KEY", key)
        .status()
        .is_ok_and(|status| status.success());
    drop(wrangler);
    served
}
