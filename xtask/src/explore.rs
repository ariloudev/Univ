//! Univ Explorer's tasks: its gate in `cargo xtask check`, and `cargo xtask explore`, which runs
//! Univ Server on the real local state with the explorer's dev server beside it.
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
