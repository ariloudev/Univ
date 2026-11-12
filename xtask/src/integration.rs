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

use univ_model::release::shards::{Database, bound_shards};

use crate::{Outcome, command};

/// The Worker's crate, from the workspace root, which holds its `wrangler.toml`.
pub(crate) const WORKER_DIR: &str = "crates/univ-server";

/// The owner secret the Worker is started with on the fixture state, for the tests. Runs on
/// the real local state never take it: their Worker reads its own from `.dev.vars`.
pub(crate) const TEST_OWNER_SECRET: &str = "integration-owner-secret";

/// The local state Univ Ingest publishes real releases to.
pub(crate) const REAL_STATE: &str = univ_model::release::REAL_LOCAL_STATE;

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
    let Some((wrangler, port)) = start_ready(root, &state, &log, Some(TEST_OWNER_SECRET)) else {
        eprintln!(
            "integration failed at: wrangler dev; its log is {}",
            log.display()
        );
        return false;
    };
    eprintln!("==> end-to-end tests");
    let passed = command(root, "cargo")
        .args(["test", "--quiet", "--package", "univ-server"])
        .args(["--tests", "--", "--ignored"])
        .env("UNIV_E2E_URL", format!("http://127.0.0.1:{port}"))
        .env("UNIV_E2E_OWNER_SECRET", TEST_OWNER_SECRET)
        .status()
        .is_ok_and(|status| status.success());
    let explorer = if passed {
        eprintln!("==> the explorer's end-to-end tests");
        crate::explore::end_to_end(root, port, TEST_OWNER_SECRET)
    } else {
        Outcome::Failed
    };
    drop(wrangler);
    if explorer == Outcome::Passed {
        eprintln!("integration passed");
    } else if explorer == Outcome::Skipped {
        eprintln!("integration passed, skipped: the explorer's end-to-end tests");
    } else if passed {
        eprintln!("integration failed at: the explorer's end-to-end tests");
    } else {
        eprintln!(
            "integration failed at: end-to-end tests; Wrangler's log is {}",
            log.display()
        );
    }
    explorer != Outcome::Failed
}

/// Runs the Worker on the real local state until it is stopped, on the port its
/// `wrangler.toml` names for development.
/// It takes the owner secret of the Worker's `.dev.vars`, and runs without one, refusing
/// every owner request, where there is none.
pub fn serve(root: &Path) -> bool {
    if crate::owner::real_owner(root).is_none() {
        eprintln!("serving all the same: owner requests will be refused");
    }
    prepared_real_state(root)
        .is_some_and(|state| succeeded(wrangler(root, "dev", &state).status(), "wrangler dev"))
}

/// The variable that names another local state for `serve`, `explore` and the benches, such as
/// a scratch release's, in place of the real one.
const STATE_VARIABLE: &str = "UNIV_LOCAL_STATE";

/// The local state the Worker runs on: the real one, or the one `UNIV_LOCAL_STATE` names, made
/// absolute, since Wrangler runs from the Worker's directory.
pub(crate) fn local_state(root: &Path) -> std::path::PathBuf {
    match std::env::var_os(STATE_VARIABLE) {
        Some(named) => {
            let state = std::path::absolute(&named).unwrap_or_else(|_| named.into());
            eprintln!(
                "on the local state {} that {STATE_VARIABLE} names",
                state.display()
            );
            state
        }
        None => root.join(REAL_STATE),
    }
}

/// The local state, its release tables and the univ-layers migrations brought up to date; none,
/// having said why.
pub(crate) fn prepared_real_state(root: &Path) -> Option<std::path::PathBuf> {
    let state = local_state(root);
    if !state.is_dir() {
        eprintln!(
            "{} holds no published release: run `univ-ingest publish` first",
            state.display()
        );
        return None;
    }
    if let Some(held) = crate::schema7::json_held(&state) {
        eprintln!(
            "{held}: JSON of D1 schema 7, which the Worker of schema 8 does not read; run \
             `univ-ingest migrate` on a clone of {} and swap it in first",
            state.display()
        );
        return None;
    }
    (release_tables(root, &state) && migrate(root, &state)).then_some(state)
}

/// Creates each table of univ-model's release schema that a database of the releases in
/// `state` lacks, the catalogue database and each object shard the Worker binds, as one built
/// before the schema grew does: its rows stay as they are, and the Worker reads the new tables
/// as holding nothing for it. The schema is only `CREATE … IF NOT EXISTS`; the schema version
/// is Univ Ingest's to record when it publishes. One database after another: Wrangler runs on
/// one local state at once fail (`SQLITE_BUSY`), whichever databases they open.
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
    let config = root.join(WORKER_DIR).join("wrangler.toml");
    let shards = match fs::read_to_string(&config) {
        Ok(toml) => bound_shards(&toml),
        Err(error) => {
            eprintln!("could not read {}: {error}", config.display());
            return false;
        }
    };
    (0..shards)
        .map(Database::Shard)
        .chain([Database::Catalogue])
        .all(|database| {
            let mut execute = wrangler(root, "d1", state);
            execute
                .args(["execute", &database.name(), "--local", "--file"])
                .arg(&file);
            let step = format!("wrangler d1 execute (release schema, {database})");
            succeeded(execute.status(), &step)
        })
}

/// Applies the univ-layers migrations to the local state in `state`.
pub(crate) fn migrate(root: &Path, state: &Path) -> bool {
    let mut migrate = wrangler(root, "d1", state);
    migrate.args(["migrations", "apply", "univ-layers", "--local"]);
    succeeded(migrate.status(), "wrangler d1 migrations apply")
}

/// `npx wrangler <subcommand> --persist-to <state>`, run from the Worker's crate directory,
/// quiet and without telemetry, as Univ Ingest runs Wrangler.
pub(crate) fn wrangler(root: &Path, subcommand: &str, state: &Path) -> Command {
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
/// port when it exits first; the running Wrangler and its port, or none. The Worker's owner
/// secret is `owner` where given, and otherwise the one Wrangler reads from the Worker's
/// `.dev.vars`.
pub(crate) fn start_ready(
    root: &Path,
    state: &Path,
    log: &Path,
    owner: Option<&str>,
) -> Option<(Wrangler, u16)> {
    for start in 1..=STARTS {
        let port = match free_port() {
            Ok(port) => port,
            Err(error) => {
                eprintln!("could not find a free port: {error}");
                return None;
            }
        };
        eprintln!("==> wrangler dev on port {port}");
        let mut wrangler = match Wrangler::start(root, state, port, log, owner) {
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
    fn start(
        root: &Path,
        state: &Path,
        port: u16,
        log: &Path,
        owner: Option<&str>,
    ) -> std::io::Result<Self> {
        let out = fs::File::create(log)?;
        let err = out.try_clone()?;
        let mut dev = wrangler(root, "dev", state);
        if let Some(secret) = owner {
            dev.args(["--var", &format!("OWNER_SECRET:{secret}")]);
        }
        let child = dev
            .args(["--port", &port.to_string()])
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
    send(port, &Exchange::get(path), timeout).map(|(status, _)| status)
}

/// One request to the Worker on `port`.
pub(crate) struct Exchange<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub bearer: Option<&'a str>,
    pub json: Option<&'a str>,
}

impl<'a> Exchange<'a> {
    pub(crate) fn get(path: &'a str) -> Self {
        Self {
            method: "GET",
            path,
            bearer: None,
            json: None,
        }
    }
}

/// Sends `exchange` over one connection, and gives the status and the body as text, read
/// until the Worker closes the connection.
pub(crate) fn send(
    port: u16,
    exchange: &Exchange,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let authorization = exchange
        .bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let body = exchange.json.unwrap_or_default();
    let content = exchange
        .json
        .map(|json| {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                json.len()
            )
        })
        .unwrap_or_default();
    let request = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\
         {authorization}{content}\r\n{body}",
        exchange.method, exchange.path
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let response = String::from_utf8_lossy(&response).into_owned();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| "no status line".to_owned())?;
    let body = response
        .split_once("\r\n\r\n")
        .map_or("", |(_, body)| body)
        .to_owned();
    Ok((status, body))
}
