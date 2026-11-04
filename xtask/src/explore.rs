//! Univ Explorer's tasks: its gate in `cargo xtask check`; its end-to-end tests, which `cargo
//! xtask integration` runs against the fixture state; `cargo xtask explore`, which runs Univ
//! Server on the real local state with the explorer's dev server beside it; and `cargo xtask
//! bench first-points`, which measures how soon the explorer draws its first points there.
//!
//! The dev server passes the explorer's reads on to the Worker, adding an extended-tier key
//! issued for the run, so that the explorer reads its tiles at the extended rate and the key
//! never reaches the browser. Each run first revokes the keys earlier runs issued, which
//! Ctrl-C leaves standing.

use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::Path;

use crate::integration::{fresh_key, start_on_real_state};
use crate::{Outcome, command};

/// The explorer's directory, relative to the workspace.
const DIR: &str = "clients/explorer";

/// Where the dev server listens, as its `vite.config.ts` fixes it.
const ADDRESS: &str = "127.0.0.1:5317";

/// The label of the keys `explore` issues.
const LABEL: &str = "explorer";

/// The explorer's locked dependencies, installed quietly.
const INSTALL: &[&str] = &["install", "--frozen-lockfile", "--silent"];

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
    match pnpm(root, INSTALL) {
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
    // Checked first: the Worker's build takes minutes, and Vite will not take another port.
    if let Err(error) = TcpListener::bind(ADDRESS) {
        eprintln!("the explorer's address {ADDRESS} is taken ({error}): stop what holds it");
        return false;
    }
    match pnpm(root, INSTALL) {
        None => {
            eprintln!("pnpm is not on the path: install it (https://pnpm.io) and run again");
            return false;
        }
        Some(false) => {
            eprintln!("pnpm could not install {DIR}'s locked dependencies; its output says why");
            return false;
        }
        Some(true) => {}
    }
    eprintln!(
        "==> Univ Server on the real local state (a first build of the Worker takes minutes)"
    );
    let log = root.join("target/explore/wrangler.log");
    let Some((wrangler, port)) = start_on_real_state(root, &log) else {
        return false;
    };
    let Some(key) = fresh_key(port, LABEL) else {
        return false;
    };
    eprintln!("==> Univ Explorer at http://{ADDRESS}/ (Ctrl-C stops both)");
    let served = match command(&root.join(DIR), "pnpm")
        .arg("dev")
        .env("UNIV_SERVER", format!("http://127.0.0.1:{port}"))
        .env("UNIV_KEY", key)
        .status()
    {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("could not run pnpm dev: {error}");
            false
        }
    };
    drop(wrangler);
    served
}

/// Runs `pnpm` with `args` in the explorer's directory, reading Univ Server at `port` with `key`.
fn pnpm_reading(root: &Path, args: &[&str], port: u16, key: &str) -> bool {
    match command(&root.join(DIR), "pnpm")
        .args(args)
        .env("UNIV_SERVER", format!("http://127.0.0.1:{port}"))
        .env("UNIV_KEY", key)
        .status()
    {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("could not run pnpm: {error}");
            false
        }
    }
}

/// The explorer's end-to-end tests against Univ Server at `port`, which serves the fixture
/// state: Playwright drives Chromium through the explorer's dev server. Read with a key of the
/// extended tier, as the server's own tests leave the anonymous limit spent. Skipped when pnpm
/// is not on the path.
pub fn end_to_end(root: &Path, port: u16) -> Outcome {
    match pnpm(root, INSTALL) {
        None => {
            eprintln!(
                "skipped: pnpm is not on the path, so the explorer's end-to-end tests did not run"
            );
            return Outcome::Skipped;
        }
        Some(false) => return Outcome::Failed,
        Some(true) => {}
    }
    let Some(key) = fresh_key(port, "explorer-e2e") else {
        return Outcome::Failed;
    };
    if pnpm_reading(root, &["e2e"], port, &key) {
        Outcome::Passed
    } else {
        Outcome::Failed
    }
}

/// Measures the explorer's first points on the real local state (`explorer.first_points.latency`):
/// 10 loads of its dev server beside Univ Server, the 95th percentile against 3 seconds.
pub fn first_points(root: &Path) -> bool {
    if pnpm(root, INSTALL) != Some(true) {
        eprintln!("bench failed at: installing the explorer's dependencies (is pnpm on the path?)");
        return false;
    }
    let log = root.join("target/bench/wrangler.log");
    let Some((wrangler, port)) = start_on_real_state(root, &log) else {
        eprintln!("bench failed at: starting Univ Server");
        return false;
    };
    let Some(key) = fresh_key(port, "bench") else {
        return false;
    };
    let measured = pnpm_reading(root, &["first-points"], port, &key);
    drop(wrangler);
    measured
}
