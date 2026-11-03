//! The publish and prune commands: a built release uploaded to local D1 and R2 through
//! Wrangler, and a release pruned from them. Each prints a plain, timestamped line per step.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::publish::{PruneOptions, PublishOptions, Wrangler, prune, publish};

/// Where Wrangler keeps local D1 and R2 by default: what `wrangler dev` reads from the
/// repository root.
pub const DEFAULT_PERSIST: &str = ".wrangler/state";
/// The Wrangler configuration naming the local D1 and R2 bindings.
pub const DEFAULT_CONFIG: &str = "wrangler.toml";

/// What a publish or prune command is given.
#[derive(Clone, Debug)]
pub struct StoreOptions {
    /// The release number.
    pub number: u32,
    /// The build output, whose `releases/{n}` holds each release.
    pub out: PathBuf,
    /// Wrangler's local state directory.
    pub persist_to: PathBuf,
    /// The Wrangler configuration.
    pub config: PathBuf,
}

/// The time now, RFC 3339 in UTC to the second.
#[must_use]
pub fn now() -> String {
    humantime::format_rfc3339_seconds(SystemTime::now()).to_string()
}

/// The directory of the publish journals in Wrangler's local state directory.
#[must_use]
pub fn journals(persist_to: &Path) -> PathBuf {
    persist_to.join("univ-ingest")
}

/// Publishes the release, giving each step's line to `line` and the closing line.
///
/// # Errors
///
/// Fails, saying why and where `latest` points, when Wrangler is missing or the release is
/// refused or not completely published.
pub fn publish_command(
    options: &StoreOptions,
    line: &mut dyn FnMut(String),
) -> Result<String, String> {
    let mut store = Wrangler::new(&options.config, &options.persist_to)?;
    let journals = journals(&options.persist_to);
    let publish_options = PublishOptions {
        out: &options.out,
        journals: &journals,
        now: &now,
    };
    let done =
        publish(&mut store, options.number, &publish_options, line).map_err(|e| e.to_string())?;
    Ok(format!(
        "Published release {} at {}: latest points to release {} (it pointed to {}); {} files \
         uploaded and checked by this run, {} by an earlier one",
        done.number, done.published_at, done.number, done.previous, done.uploaded, done.journaled
    ))
}

/// Prunes the release, giving each step's line to `line` and the closing line.
///
/// # Errors
///
/// Fails, saying why, when Wrangler is missing or the release is refused or not completely
/// pruned.
pub fn prune_command(
    options: &StoreOptions,
    line: &mut dyn FnMut(String),
) -> Result<String, String> {
    let mut store = Wrangler::new(&options.config, &options.persist_to)?;
    let journals = journals(&options.persist_to);
    let prune_options = PruneOptions {
        out: &options.out,
        journals: &journals,
        now: &now,
    };
    let done =
        prune(&mut store, options.number, &prune_options, line).map_err(|e| e.to_string())?;
    let when = if done.already {
        "already pruned at"
    } else {
        "pruned at"
    };
    Ok(format!(
        "Release {} {when} {}: {} R2 files and {} rows deleted; it stays in the list of \
         releases with its pruning time, and the id ledger is kept; latest points to {}",
        done.number, done.pruned_at, done.files, done.rows, done.latest
    ))
}
