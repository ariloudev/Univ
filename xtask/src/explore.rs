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

use crate::integration::{fresh_key, with_real_server};
use crate::{Outcome, command};

/// The explorer's directory, relative to the workspace.
const DIR: &str = "clients/explorer";

/// Where the dev server listens, as its `vite.config.ts` fixes it.
const ADDRESS: &str = "127.0.0.1:5317";

/// The explorer's locked dependencies, installed quietly.
const INSTALL: &[&str] = &["install", "--frozen-lockfile", "--silent"];

/// The Chromium that Playwright drives, installed where it is missing.
const BROWSER: &[&str] = &["exec", "playwright", "install", "chromium"];

/// Univ Server for the explorer to read: its port, and an API key for the dev server to add.
struct Reading<'a> {
    port: u16,
    key: &'a str,
}

/// Runs `pnpm` with `args` in the explorer's directory, reading Univ Server as `reading` says;
/// none when pnpm is not on the path.
fn pnpm(root: &Path, args: &[&str], reading: Option<&Reading>) -> Option<bool> {
    let mut pnpm = command(&root.join(DIR), "pnpm");
    pnpm.args(args);
    if let Some(Reading { port, key }) = reading {
        pnpm.env("UNIV_SERVER", format!("http://127.0.0.1:{port}"))
            .env("UNIV_KEY", key);
    }
    match pnpm.status() {
        Ok(status) => Some(status.success()),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("could not run pnpm: {error}");
            Some(false)
        }
    }
}

/// Installs the explorer's locked dependencies, and Chromium where `browser`; the outcome to
/// end on where that did not happen: skipped without pnpm, failed otherwise.
fn installed(root: &Path, browser: bool) -> Result<(), Outcome> {
    match pnpm(root, INSTALL, None) {
        None => {
            eprintln!("skipped: pnpm is not on the path (https://pnpm.io), so {DIR} did not run");
            Err(Outcome::Skipped)
        }
        Some(false) => {
            eprintln!("pnpm could not install {DIR}'s locked dependencies; its output says why");
            Err(Outcome::Failed)
        }
        Some(true) if browser && pnpm(root, BROWSER, None) != Some(true) => {
            eprintln!("Playwright could not install Chromium; its output says why");
            Err(Outcome::Failed)
        }
        Some(true) => Ok(()),
    }
}

/// The outcome of a pnpm run.
fn outcome(ran: Option<bool>) -> Outcome {
    if ran == Some(true) {
        Outcome::Passed
    } else {
        Outcome::Failed
    }
}

/// The explorer's gate: its locked dependencies installed, then its own check (typecheck, unit
/// tests with coverage, fallow, build). Skipped when pnpm is not on the path.
pub fn gate(root: &Path) -> Outcome {
    match installed(root, false) {
        Ok(()) => outcome(pnpm(root, &["check"], None)),
        Err(ended) => ended,
    }
}

/// The explorer's end-to-end tests against Univ Server at `port`, which serves the fixture
/// state: Playwright drives Chromium through the explorer's dev server. Read with a key of the
/// extended tier, as the server's own tests leave the anonymous limit spent. Skipped when pnpm
/// is not on the path.
pub fn end_to_end(root: &Path, port: u16) -> Outcome {
    if let Err(ended) = installed(root, true) {
        return ended;
    }
    let Some(key) = fresh_key(port, "explorer-e2e") else {
        return Outcome::Failed;
    };
    outcome(pnpm(root, &["e2e"], Some(&Reading { port, key: &key })))
}

/// Runs Univ Server on the real local state and the explorer beside it, until interrupted.
pub fn explore(root: &Path) -> bool {
    // Checked first: the Worker's build takes minutes, and Vite will not take another port.
    if let Err(error) = TcpListener::bind(ADDRESS) {
        eprintln!("the explorer's address {ADDRESS} is taken ({error}): stop what holds it");
        return false;
    }
    if installed(root, false).is_err() {
        return false;
    }
    eprintln!(
        "==> Univ Server on the real local state (a first build of the Worker takes minutes)"
    );
    let log = root.join("target/explore/wrangler.log");
    with_real_server(root, &log, "explorer", |port, key| {
        eprintln!("==> Univ Explorer at http://{ADDRESS}/ (Ctrl-C stops both)");
        pnpm(root, &["dev"], Some(&Reading { port, key })) == Some(true)
    })
}

/// Measures the explorer's first points on the real local state
/// (`explorer.first_points.latency`): a load to warm up, then 10 of its dev server beside Univ
/// Server, the 95th percentile against 3 seconds.
pub fn first_points(root: &Path) -> bool {
    if installed(root, true).is_err() {
        return false;
    }
    let log = root.join("target/bench/wrangler.log");
    with_real_server(root, &log, "bench", |port, key| {
        pnpm(root, &["first-points"], Some(&Reading { port, key })) == Some(true)
    })
}
