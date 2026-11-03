//! `cargo xtask integration`: the end-to-end tests of Univ Server against the real Worker.
//!
//! It writes the fixture state (ingest's fixture releases, the first pruned) into a fresh
//! directory, applies the univ-layers migrations there, starts `wrangler dev` on a free port,
//! runs the ignored tests of `crates/univ-server/tests/` against it, and stops Wrangler, also
//! when a step fails. `cargo xtask serve` runs the same Worker on the real local state.
//!
//! Wrangler runs from the Worker's crate directory, where its `wrangler.toml` lies, as it would
//! by hand; `npx` finds it in the workspace's `node_modules`.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::command;

/// The Worker's crate, from the workspace root, which holds its `wrangler.toml`.
const WORKER_DIR: &str = "crates/univ-server";

/// The owner secret the Worker is started with in the tests.
const OWNER_SECRET: &str = "integration-owner-secret";

/// The local state Univ Ingest publishes real releases to.
pub(crate) const REAL_STATE: &str = ".cache/univ-ingest/wrangler";

/// How long the Worker may take to build and answer: a cold release build of the wasm.
const READY_WITHIN: Duration = Duration::from_secs(600);

/// How many times Wrangler is started on a fresh port when it exits before answering, as it
/// does when another process took the port first.
const STARTS: u32 = 3;

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
    let log = work.join("wrangler.log");
    let Some((wrangler, port)) = start_ready(root, &state, &log) else {
        eprintln!(
            "integration failed at: wrangler dev; its log is {}",
            log.display()
        );
        return false;
    };
    eprintln!("==> end-to-end tests");
    let passed = command(root, "cargo")
        .args(["test", "--quiet", "--package", "univ-server"])
        .args(["--test", "e2e", "--", "--ignored"])
        .env("UNIV_E2E_URL", format!("http://127.0.0.1:{port}"))
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

/// Runs the Worker on the real local state until it is stopped, on the port its
/// `wrangler.toml` names for development.
pub fn serve(root: &Path) -> bool {
    let state = root.join(REAL_STATE);
    if !state.is_dir() {
        eprintln!(
            "{} holds no published release: run `univ-ingest publish` first",
            state.display()
        );
        return false;
    }
    release_tables(root, &state)
        && migrate(root, &state)
        && succeeded(wrangler(root, "dev", &state).status(), "wrangler dev")
}

/// Creates each table of univ-model's release schema that the release database in `state`
/// lacks, as a release built before the schema grew does: its rows stay as they are, and the
/// Worker reads the new tables as holding nothing for it. The schema is only `CREATE … IF NOT
/// EXISTS`; the schema version is Univ Ingest's to record when it publishes.
pub(crate) fn release_tables(root: &Path, state: &Path) -> bool {
    let file = root.join("target/serve/release-schema.sql");
    let written = file
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&file, univ_model::release::SCHEMA));
    if let Err(error) = written {
        eprintln!("could not write {}: {error}", file.display());
        return false;
    }
    let mut execute = wrangler(root, "d1", state);
    execute
        .args(["execute", "univ", "--local", "--file"])
        .arg(&file);
    succeeded(execute.status(), "wrangler d1 execute (release schema)")
}

/// Applies the univ-layers migrations to the local state in `state`.
pub(crate) fn migrate(root: &Path, state: &Path) -> bool {
    let mut migrate = wrangler(root, "d1", state);
    migrate.args(["migrations", "apply", "univ-layers", "--local"]);
    succeeded(migrate.status(), "wrangler d1 migrations apply")
}

/// `npx wrangler <subcommand> --persist-to <state>`, run from the Worker's crate directory,
/// quiet and without telemetry, as Univ Ingest runs Wrangler.
fn wrangler(root: &Path, subcommand: &str, state: &Path) -> Command {
    let mut wrangler = command(&root.join(WORKER_DIR), "npx");
    wrangler
        .args(["--no-install", "wrangler", subcommand])
        .arg("--persist-to")
        .arg(state)
        .env("WRANGLER_SEND_METRICS", "false")
        .env("NO_COLOR", "1");
    wrangler
}

/// Whether a command ran and succeeded, saying why not when it did not.
fn succeeded(status: std::io::Result<std::process::ExitStatus>, what: &str) -> bool {
    match status {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("could not run {what}: {error} (run `npm install` first)");
            false
        }
    }
}

/// Starts `wrangler dev` on a free port and waits until it answers, starting it again on a new
/// port when it exits first; the running Wrangler and its port, or none.
pub(crate) fn start_ready(root: &Path, state: &Path, log: &Path) -> Option<(Wrangler, u16)> {
    for start in 1..=STARTS {
        let port = match free_port() {
            Ok(port) => port,
            Err(error) => {
                eprintln!("could not find a free port: {error}");
                return None;
            }
        };
        eprintln!("==> wrangler dev on port {port}");
        let mut wrangler = match Wrangler::start(root, state, port, log) {
            Ok(wrangler) => wrangler,
            Err(error) => {
                eprintln!("could not start wrangler dev: {error}");
                return None;
            }
        };
        match wrangler.wait_ready(port) {
            Ready::Answering => return Some((wrangler, port)),
            Ready::Exited if start < STARTS => {
                eprintln!("wrangler dev exited before it answered; starting it again");
            }
            Ready::Exited => eprintln!("wrangler dev exited before it answered, {STARTS} times"),
            Ready::TimedOut => {
                eprintln!(
                    "wrangler dev did not answer within {} s",
                    READY_WITHIN.as_secs()
                );
                return None;
            }
        }
    }
    None
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

/// How waiting for Wrangler ended.
enum Ready {
    Answering,
    Exited,
    TimedOut,
}

/// A running `wrangler dev`, in a process group of its own so that stopping it stops the
/// workerd processes it started too. Dropping it stops it, and so does Ctrl-C or a TERM to
/// xtask, which would otherwise reach xtask alone and leave the group running.
pub(crate) struct Wrangler {
    child: Child,
}

/// The process group of the running Wrangler, or 0 when none runs.
static GROUP: AtomicU32 = AtomicU32::new(0);

/// Stops the process group `group`.
fn stop_group(group: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", "--", &format!("-{group}")])
        .status();
}

/// Stops the running Wrangler's group when xtask is interrupted, then exits as interrupted.
pub fn stop_on_interrupt() {
    let installed = ctrlc::set_handler(|| {
        let group = GROUP.swap(0, Ordering::SeqCst);
        if group != 0 {
            stop_group(group);
        }
        std::process::exit(130);
    });
    if let Err(error) = installed {
        eprintln!("Ctrl-C will not stop Wrangler: {error}");
    }
}

impl Wrangler {
    fn start(root: &Path, state: &Path, port: u16, log: &Path) -> std::io::Result<Self> {
        let out = fs::File::create(log)?;
        let err = out.try_clone()?;
        let child = wrangler(root, "dev", state)
            .args(["--port", &port.to_string()])
            .args(["--var", &format!("OWNER_SECRET:{OWNER_SECRET}")])
            .arg("--show-interactive-dev-session=false")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .process_group(0)
            .spawn()?;
        GROUP.store(child.id(), Ordering::SeqCst);
        Ok(Self { child })
    }

    /// Waits until the Worker answers its OpenAPI document, Wrangler exits, or the time runs
    /// out.
    fn wait_ready(&mut self, port: u16) -> Ready {
        let started = Instant::now();
        while started.elapsed() < READY_WITHIN {
            if answers(port) {
                eprintln!("ready after {} s", started.elapsed().as_secs());
                return Ready::Answering;
            }
            if !matches!(self.child.try_wait(), Ok(None)) {
                return Ready::Exited;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Ready::TimedOut
    }
}

impl Drop for Wrangler {
    fn drop(&mut self) {
        GROUP.store(0, Ordering::SeqCst);
        stop_group(self.child.id());
        let _ = self.child.wait();
    }
}

/// Whether the Worker on `port` answers `GET /v1/openapi.json` with 200.
fn answers(port: u16) -> bool {
    status(port, "/v1/openapi.json", Duration::from_secs(5)) == Ok(200)
}

/// The status of `GET path` on the Worker at `port`, its answer read to the end within
/// `timeout`.
pub(crate) fn status(port: u16, path: &str, timeout: Duration) -> Result<u16, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let line = response.split(|b| *b == b'\n').next().unwrap_or_default();
    String::from_utf8_lossy(line)
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| "no status line".to_owned())
}
