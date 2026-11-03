//! The publish and prune commands: a built release uploaded to local D1 and R2 through
//! Wrangler, and a release pruned from them. Each prints a plain, timestamped line per step.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::publish::{Store, StoreOptions, Wrangler, ingest_dir, prune, publish};
use crate::release::release_dir;

/// Where Wrangler keeps local D1 and R2 by default: what `wrangler dev` reads from the
/// repository root.
pub const DEFAULT_PERSIST: &str = ".wrangler/state";
/// The Wrangler configuration naming the local D1 and R2 bindings.
pub const DEFAULT_CONFIG: &str = "wrangler.toml";

/// What a publish or prune command is given.
#[derive(Clone, Debug)]
pub struct StoreRun {
    /// The release number.
    pub number: u32,
    /// The build output, whose `releases/{n}` holds each release.
    pub out: PathBuf,
    /// The state directory, which records each release built.
    pub state: PathBuf,
    /// Wrangler's local state directory.
    pub persist_to: PathBuf,
    /// The Wrangler configuration.
    pub config: PathBuf,
}

/// A publish or prune of one release against a store, giving each step's line to its last
/// argument and its closing line as its outcome.
pub type StoreCommand =
    fn(&mut dyn Store, u32, &StoreOptions<'_>, &mut dyn FnMut(String)) -> Result<String, String>;

/// The time now, RFC 3339 in UTC to the second.
#[must_use]
pub fn now() -> String {
    humantime::format_rfc3339_seconds(SystemTime::now()).to_string()
}

/// Runs `command` against local D1 and R2 through Wrangler, giving each step's line to `line`.
///
/// # Errors
///
/// Fails, saying why, when Wrangler is missing or the command fails.
pub fn store_command(
    run: &StoreRun,
    command: StoreCommand,
    line: &mut dyn FnMut(String),
) -> Result<String, String> {
    let mut store = Wrangler::new(&run.config, &run.persist_to)?;
    let journals = ingest_dir(&run.persist_to);
    let options = StoreOptions {
        out: &run.out,
        state: &run.state,
        journals: &journals,
        now: &now,
    };
    command(&mut store, run.number, &options, line)
}

/// Publishes the release, giving its closing line.
///
/// # Errors
///
/// Fails, saying why and where `latest` points, when the release is refused or not completely
/// published.
pub fn publish_command(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    line: &mut dyn FnMut(String),
) -> Result<String, String> {
    let done = publish(store, number, options, line).map_err(|e| e.to_string())?;
    Ok(format!(
        "Published release {} at {}: latest points to release {} (it pointed to {}); {} files \
         uploaded and checked by this run, {} by an earlier one",
        done.number, done.published_at, done.number, done.previous, done.uploaded, done.journaled
    ))
}

/// Prunes the release, giving its closing line.
///
/// # Errors
///
/// Fails, saying why, when the release is refused or not completely pruned.
pub fn prune_command(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    line: &mut dyn FnMut(String),
) -> Result<String, String> {
    let done = prune(store, number, options, line).map_err(|e| e.to_string())?;
    let build = match &done.build {
        Some(dir) => format!("its local build {} was deleted", dir.display()),
        None => format!(
            "no local build was left at {}",
            release_dir(options.out, number).display()
        ),
    };
    if !done.listed {
        return Ok(format!(
            "Release {} was never listed in D1: the {} R2 keys its journal and manifest name \
             were deleted at {}, and {build}; latest points to {}",
            done.number, done.files, done.pruned_at, done.latest
        ));
    }
    let when = if done.already {
        "already pruned at"
    } else {
        "pruned at"
    };
    Ok(format!(
        "Release {} {when} {}: {} R2 files and {} rows deleted, and {build}; it stays in the \
         list of releases with its pruning time, and the id ledger is kept; latest points to {}",
        done.number, done.pruned_at, done.files, done.rows, done.latest
    ))
}
