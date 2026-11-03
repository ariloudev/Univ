//! `cargo xtask integration`: the end-to-end tests of Univ Server against the real Worker.
//!
//! It writes the fixture state (ingest's fixture releases, the first pruned) into a fresh
//! directory, applies the univ-layers migrations there, starts `wrangler dev` on a free port,
//! runs the ignored tests of `crates/univ-server/tests/` against it, and stops Wrangler, also
//! when a step fails. `cargo xtask serve` runs the same Worker on the real local state.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use crate::{command, run};

/// The Worker's Wrangler configuration, from the workspace root.
const CONFIG: &str = "crates/univ-server/wrangler.toml";

/// The owner secret the Worker is started with in the tests.
const OWNER_SECRET: &str = "integration-owner-secret";

/// The local state Univ Ingest publishes real releases to.
const REAL_STATE: &str = ".cache/univ-ingest/wrangler";

/// The port `serve` listens on: one of its own, since 8787, Wrangler's default, is often taken
/// by another project's Worker.
const SERVE_PORT: &str = "8790";

/// How long the Worker may take to build and answer: a cold release build of the wasm.
const READY_WITHIN: Duration = Duration::from_secs(600);

/// Runs the end-to-end tests; true when every step passed.
pub fn integration(root: &Path) -> bool {
    let work = root.join("target/integration");
    let state = work.join("state");
    if let Err(error) = fresh_dir(&state) {
        eprintln!("could not empty {}: {error}", state.display());
        return false;
    }
    eprintln!("==> fixture state in {}", state.display());
    let fixture = command(root, "cargo")
        .args(["test", "--quiet", "--package", "univ-ingest"])
        .args(["--test", "fixture_state", "--", "--ignored"])
        .env("UNIV_FIXTURE_STATE", &state)
        .status();
    if !fixture.is_ok_and(|status| status.success()) {
        eprintln!("integration failed at: fixture state");
        return false;
    }
    if !migrate(root, &state) {
        eprintln!("integration failed at: univ-layers migrations");
        return false;
    }
    let port = match free_port() {
        Ok(port) => port,
        Err(error) => {
            eprintln!("could not find a free port: {error}");
            return false;
        }
    };
    eprintln!("==> wrangler dev on port {port}");
    let log = work.join("wrangler.log");
    let wrangler = match Wrangler::start(root, &state, port, &log) {
        Ok(wrangler) => wrangler,
        Err(error) => {
            eprintln!("could not start wrangler dev: {error}");
            return false;
        }
    };
    if let Err(error) = wrangler.wait_ready(port) {
        eprintln!("{error}; its log is {}", log.display());
        return false;
    }
    eprintln!("==> end-to-end tests");
    let passed = command(root, "cargo")
        .args(["test", "--quiet", "--package", "univ-server"])
        .args(["--test", "e2e", "--", "--ignored"])
        .env("UNIV_E2E_URL", format!("http://127.0.0.1:{port}"))
        .env("UNIV_E2E_OWNER_SECRET", OWNER_SECRET)
        .status()
        .is_ok_and(|status| status.success());
    drop(wrangler);
    if passed {
        eprintln!("integration passed");
    } else {
        eprintln!(
            "integration failed at: end-to-end tests; Wrangler's log is {}",
            log.display()
        );
    }
    passed
}

/// Runs the Worker on the real local state until it is stopped.
pub fn serve(root: &Path) -> bool {
    let state = root.join(REAL_STATE);
    if !state.is_dir() {
        eprintln!(
            "{} holds no published release: run `univ-ingest publish` first",
            state.display()
        );
        return false;
    }
    if !migrate(root, &state) {
        return false;
    }
    let state = state.display().to_string();
    run(
        root,
        "npx",
        &[
            "--no-install",
            "wrangler",
            "dev",
            "--config",
            CONFIG,
            "--persist-to",
            &state,
            "--ip",
            "127.0.0.1",
            "--port",
            SERVE_PORT,
        ],
    )
}

/// Applies the univ-layers migrations to the local state in `state`.
fn migrate(root: &Path, state: &Path) -> bool {
    let state = state.display().to_string();
    run(
        root,
        "npx",
        &[
            "--no-install",
            "wrangler",
            "d1",
            "migrations",
            "apply",
            "univ-layers",
            "--local",
            "--persist-to",
            &state,
            "--config",
            CONFIG,
        ],
    )
}

/// Removes `dir` and everything below it, and creates it empty.
fn fresh_dir(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)
}

/// A port on the loopback interface that nothing listens on now.
fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// A running `wrangler dev`, in a process group of its own so that stopping it stops the
/// workerd processes it started too. Dropping it stops it.
struct Wrangler {
    child: Child,
}

impl Wrangler {
    fn start(root: &Path, state: &Path, port: u16, log: &PathBuf) -> std::io::Result<Self> {
        let out = fs::File::create(log)?;
        let err = out.try_clone()?;
        let child = command(root, "npx")
            .args(["--no-install", "wrangler", "dev", "--config", CONFIG])
            .arg("--persist-to")
            .arg(state)
            .args(["--ip", "127.0.0.1", "--port", &port.to_string()])
            .args(["--var", &format!("OWNER_SECRET:{OWNER_SECRET}")])
            .arg("--show-interactive-dev-session=false")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .process_group(0)
            .spawn()?;
        Ok(Self { child })
    }

    /// Waits until the Worker answers its OpenAPI document, or fails when Wrangler exits or
    /// the time runs out.
    fn wait_ready(&self, port: u16) -> Result<(), String> {
        let started = Instant::now();
        while started.elapsed() < READY_WITHIN {
            if answers(port) {
                eprintln!("ready after {} s", started.elapsed().as_secs());
                return Ok(());
            }
            if !self.running() {
                return Err("wrangler dev exited before it answered".to_owned());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(format!(
            "wrangler dev did not answer within {} s",
            READY_WITHIN.as_secs()
        ))
    }

    fn running(&self) -> bool {
        let pid = self.child.id().to_string();
        std::process::Command::new("kill")
            .args(["-0", &pid])
            .status()
            .is_ok_and(|status| status.success())
    }
}

impl Drop for Wrangler {
    fn drop(&mut self) {
        let group = format!("-{}", self.child.id());
        let _ = std::process::Command::new("kill")
            .args(["-TERM", "--", &group])
            .status();
        let _ = self.child.wait();
    }
}

/// Whether the Worker on `port` answers `GET /v1/openapi.json` with 200.
fn answers(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /v1/openapi.json HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut head = [0_u8; 12];
    stream.read_exact(&mut head).is_ok() && head.ends_with(b"200")
}
