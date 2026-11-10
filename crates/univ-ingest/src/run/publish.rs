//! The publish and prune commands: a built release uploaded to local D1 and R2 through
//! Wrangler, and a release pruned from them, each shown on the run display as a stage, a step
//! per file or step, and a summary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use univ_model::release::keys;

use crate::display::Outcome;
use crate::events::{Count, Event, EventSink, Stage};
use crate::publish::{
    FileDone, Progress, Pruned, Published, Store, StoreOptions, Wrangler, ingest_dir, migrate_with,
    prune_with, publish_with,
};
use crate::release::release_dir;

/// The local D1 and R2 that releases are published to: the real local state.
pub const DEFAULT_PERSIST: &str = univ_model::release::REAL_LOCAL_STATE;
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

/// A command against local D1 and R2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreCommand {
    Publish,
    Prune,
    Migrate,
}

impl StoreCommand {
    /// The stage the command is shown as.
    #[must_use]
    pub fn stage(self) -> Stage {
        match self {
            Self::Publish => Stage::Publish,
            Self::Prune => Stage::Prune,
            Self::Migrate => Stage::Migrate,
        }
    }
}

/// A publish or prune shown on the run display: each step and file as an event, as soon as it is
/// done, with what the files came to, and the owner's stop flag.
pub struct Shown<'a> {
    sink: &'a dyn EventSink,
    stage: Stage,
    stop: &'a AtomicBool,
    /// The files done by this run of each kind, in the order first seen.
    files: Vec<(&'static str, u64)>,
    /// The rows of the release each table holds once checked by this run, in every database,
    /// in the order first checked.
    tables: Vec<(&'static str, u64)>,
}

impl<'a> Shown<'a> {
    /// The progress of `stage` on `sink`, stopping once `stop` is set.
    #[must_use]
    pub fn new(sink: &'a dyn EventSink, stage: Stage, stop: &'a AtomicBool) -> Self {
        Self {
            sink,
            stage,
            stop,
            files: Vec::new(),
            tables: Vec::new(),
        }
    }

    fn ended(&self, counts: Vec<Count>) {
        self.sink.emit(Event::StageEnded {
            stage: self.stage,
            counts,
        });
    }
}

/// The kind of a release file, by the directory its path starts with, as a count names it.
fn kind(path: &str) -> &'static str {
    match path.split('/').next() {
        Some("d1") => "D1 files",
        Some("grids") => "grid files",
        Some("rules") => "rules files",
        Some("tiles") => "tile files",
        Some(keys::MANIFEST) => "manifest files",
        _ => "other files",
    }
}

impl Progress for Shown<'_> {
    fn started(&mut self, items: usize) {
        self.sink.emit(Event::StageStarted {
            stage: self.stage,
            items: Some(items as u64),
        });
    }

    fn line(&mut self, line: String) {
        self.sink.emit(Event::Stepped {
            stage: self.stage,
            step: line,
        });
    }

    fn note(&mut self, line: String) {
        self.sink.emit(Event::Noted {
            stage: self.stage,
            line,
        });
    }

    fn file(&mut self, file: FileDone<'_>) {
        let kind = kind(file.path);
        match self.files.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => self.files.push((kind, 1)),
        }
        if let Some((table, rows)) = file.rows {
            match self.tables.iter_mut().find(|(t, _)| *t == table) {
                Some((_, n)) => *n += rows,
                None => self.tables.push((table, rows)),
            }
        }
        self.sink.emit(Event::Uploaded {
            stage: self.stage,
            step: file.line,
            bytes: file.bytes,
        });
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// The time now, RFC 3339 in UTC to the second.
#[must_use]
pub fn now() -> String {
    humantime::format_rfc3339_seconds(SystemTime::now()).to_string()
}

/// Runs `command` on release `run.number` against local D1 and R2 through Wrangler, showing it
/// on `sink`, and stopping after the unit in hand once `stop` is set. It gives how the run
/// ended.
pub fn store_command(
    run: &StoreRun,
    command: StoreCommand,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Outcome {
    let stopped = |reason: String| Outcome::Stopped {
        stage: command.stage(),
        reason,
    };
    let mut store = match Wrangler::new(&run.config, &run.persist_to) {
        Ok(store) => store,
        Err(error) => return stopped(error),
    };
    let journals = ingest_dir(&run.persist_to);
    let options = StoreOptions {
        out: &run.out,
        state: &run.state,
        journals: &journals,
        now: &now,
    };
    let mut shown = Shown::new(sink, command.stage(), stop);
    let done = match command {
        StoreCommand::Publish => publish_command(&mut store, run.number, &options, &mut shown),
        StoreCommand::Prune => prune_command(&mut store, run.number, &options, &mut shown),
        StoreCommand::Migrate => migrate_command(&mut store, &journals, &mut shown),
    };
    done.unwrap_or_else(stopped)
}

/// Brings the store under shared rows, ending its stage with whether there was anything to do.
///
/// # Errors
///
/// Fails, saying why, when D1 or R2 refuses a step.
fn migrate_command(
    store: &mut dyn Store,
    journals: &std::path::Path,
    shown: &mut Shown<'_>,
) -> Result<Outcome, String> {
    let migrated = migrate_with(store, journals, shown)?;
    shown.ended(Vec::new());
    Ok(Outcome::Migrated(migrated))
}

/// Publishes the release, ending its stage with what it uploaded: the files of each kind, the
/// files an earlier run uploaded, and the rows of each table it checked.
///
/// # Errors
///
/// Fails, saying why and where `latest` points, when the release is refused or not completely
/// published.
pub fn publish_command(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    shown: &mut Shown<'_>,
) -> Result<Outcome, String> {
    let done = publish_with(store, number, options, shown).map_err(|e| e.to_string())?;
    shown.note(published_line(&done));
    let mut counts: Vec<Count> = shown
        .files
        .iter()
        .map(|(k, n)| Count::new(*n, *k))
        .collect();
    if done.journaled > 0 {
        counts.push(Count::new(
            done.journaled as u64,
            "files uploaded by an earlier run",
        ));
    }
    if done.held > 0 {
        counts.push(Count::new(
            done.held as u64,
            "files its parent holds already",
        ));
    }
    counts.extend(
        shown
            .tables
            .iter()
            .map(|(table, rows)| Count::new(*rows, format!("{table} rows"))),
    );
    shown.ended(counts);
    Ok(Outcome::Published(u64::from(done.number)))
}

fn published_line(done: &Published) -> String {
    format!(
        "Published release {} at {}: latest points to release {} (it pointed to {}); {} files \
         uploaded and checked by this run, {} by an earlier one, {} held already by its parent",
        done.number,
        done.published_at,
        done.number,
        done.previous,
        done.uploaded,
        done.journaled,
        done.held
    )
}

/// Prunes the release, ending its stage with what it deleted.
///
/// # Errors
///
/// Fails, saying why, when the release is refused or not completely pruned.
pub fn prune_command(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    shown: &mut Shown<'_>,
) -> Result<Outcome, String> {
    let done = prune_with(store, number, options, shown).map_err(|e| e.to_string())?;
    shown.note(pruned_line(&done, options));
    shown.ended(vec![
        Count::new(done.files as u64, "R2 files deleted"),
        Count::new(done.rows, "rows deleted"),
        Count::new(u64::from(done.build.is_some()), "local builds deleted"),
    ]);
    Ok(Outcome::Pruned(u64::from(done.number)))
}

fn pruned_line(done: &Pruned, options: &StoreOptions<'_>) -> String {
    let build = match &done.build {
        Some(dir) => format!("its local build {} was deleted", dir.display()),
        None => format!(
            "no local build was left at {}",
            release_dir(options.out, done.number).display()
        ),
    };
    if !done.listed {
        return format!(
            "Release {} was never listed in D1: the {} R2 keys its journal and manifest name \
             were deleted at {}, and {build}; latest points to {}",
            done.number, done.files, done.pruned_at, done.latest
        );
    }
    let when = if done.already {
        "already pruned at"
    } else {
        "pruned at"
    };
    format!(
        "Release {} {when} {}: {} R2 files and {} rows deleted, and {build}; it stays in the \
         list of releases with its pruning time, and the id ledger is kept; latest points to {}",
        done.number, done.pruned_at, done.files, done.rows, done.latest
    )
}
