//! Univ Ingest: the command-line tool that downloads public sky data, normalizes it, resolves
//! catalogue entries into objects and publishes the result as a release that Univ Server serves.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use clap::{Args, Parser, Subcommand};
use univ_ingest::acquire::Acquired;
use univ_ingest::display::{Outcome, RunDisplay, stage_name};
use univ_ingest::events::{Event, EventSink, Stage};
use univ_ingest::fetch::{Fetcher, RetryPolicy, SystemClock};
use univ_ingest::lock::StateLock;
use univ_ingest::normalize::Normalized;
use univ_ingest::run::{
    BuildOptions, DEFAULT_BUILD, DEFAULT_CONFIG, DEFAULT_PERSIST, DEFAULT_TILES, FetchOptions,
    MatchOptions, RunReport, StoreCommand, StoreRun, TileOptions, build_stage, display_for_stderr,
    drawing, fetch, interrupted, match_stage_until, normalize_stage_until, now, owner_interrupt,
    select, store_command, tiles_stage, until_set,
};
use univ_ingest::source::SourceDefinition;
use univ_ingest::state::DEFAULT_STATE;
use univ_model::redshift::{CmbCorrection, DipoleModel};

/// Exit status of a run the owner interrupted, as for a shell's SIGINT.
const INTERRUPTED: u8 = 130;

#[derive(Parser)]
#[command(
    version,
    about = "Fetches and builds Univ releases from public sky data"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetches the sources into the cache, continuing whatever an earlier run left.
    Fetch(FetchArgs),
    /// Fetches the sources, or with --offline takes them from the cache, and normalizes each
    /// table of rows into catalogue entries.
    Normalize(FetchArgs),
    /// Fetches and normalizes the sources, then cross-matches the entries into objects with
    /// their preferred measurements, stable ids and positions, and resolves the researched
    /// structures' members. The ids it would give are written to the state directory's
    /// `pending/`; no release is built.
    Match(MatchArgs),
    /// Fetches, normalizes and cross-matches the sources, then lays out each scale domain's
    /// placed objects as an octree of tiles under `rules/tiles.yaml`, written with a JSON tile
    /// manifest per domain under the output directory; no release is built.
    Tiles(TilesArgs),
    /// Fetches, normalizes and cross-matches the sources and lays out their tiles, then builds
    /// the next release under the output directory's `releases/{n}`: its D1 rows as SQL files,
    /// its tiles, volume grids and rules, and a manifest of every file with its SHA-256. The
    /// release is checked before it is complete; only a complete release uses up its number and
    /// takes the cross-match's pending state into the state directory.
    Build(BuildArgs),
    /// Publishes a complete release to the local D1 and R2 that `wrangler dev` reads: its
    /// files to R2 and its tables to D1, each checked against its SHA-256 in the release
    /// manifest or by its rows, and only then moves latest to it in one write. An interrupted
    /// publish continues from its journal; a release already published is refused.
    Publish(StoreArgs),
    /// Deletes a release's R2 files and D1 rows, keeping it in the list of releases with its
    /// pruning time and keeping the id ledger, so every id still resolves, and last its local
    /// build in the build output. The release latest points to is refused.
    Prune(StoreArgs),
}

#[derive(Args)]
struct StoreArgs {
    /// The release number.
    number: u32,
    /// The build output, whose `releases/{n}` holds the release.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_BUILD)]
    out: PathBuf,
    /// The state directory, which records each release built; a release it does not record is
    /// not published.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_STATE)]
    state: PathBuf,
    /// Wrangler's local state directory, as `wrangler dev --persist-to` takes it.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_PERSIST)]
    persist_to: PathBuf,
    /// The Wrangler configuration naming the local D1 and R2 bindings; Wrangler is run from
    /// the `node_modules` beside it.
    #[arg(long, value_name = "FILE", default_value = DEFAULT_CONFIG)]
    config: PathBuf,
}

/// Runs the publish or prune `command` on the run display. A first Ctrl-C stops it once the unit
/// in hand is uploaded and checked, as the journal then lists it; a second stops it at once.
async fn on_store(args: StoreArgs, command: StoreCommand) -> ExitCode {
    let number = args.number;
    let run = StoreRun {
        number,
        out: args.out,
        state: args.state,
        persist_to: args.persist_to,
        config: args.config,
    };
    let display = display_for_stderr();
    let stop = Arc::new(AtomicBool::new(false));
    let (shown, flag) = (display.clone(), stop.clone());
    let work =
        tokio::task::spawn_blocking(move || store_command(&run, command, shown.as_ref(), &flag));
    let mut work = std::pin::pin!(drawing(display.as_ref(), work));
    let first = tokio::select! {
        done = &mut work => Some(done),
        () = owner_interrupt(tokio::signal::ctrl_c()) => None,
    };
    let interrupted_run = first.is_none();
    let done = if let Some(done) = first {
        done
    } else {
        stop.store(true, Ordering::Relaxed);
        display.emit(Event::Stopping {
            what: stopping(command, number),
        });
        tokio::select! {
            done = &mut work => done,
            () = owner_interrupt(tokio::signal::ctrl_c()) => {
                display.finish(vec![], Outcome::Stopped {
                    stage: command.stage(),
                    reason: "stopped at once by the owner".to_owned(),
                });
                // Returning would have the runtime wait for the unit in hand; the journal lists
                // only the units done, so the next run uploads that one again.
                std::process::exit(i32::from(INTERRUPTED));
            }
        }
    };
    let outcome = done.unwrap_or_else(|e| Outcome::Stopped {
        stage: command.stage(),
        reason: e.to_string(),
    });
    let failed = matches!(outcome, Outcome::Stopped { .. });
    display.finish(vec![], outcome);
    if interrupted_run && failed {
        ExitCode::from(INTERRUPTED)
    } else {
        ExitCode::from(u8::from(failed))
    }
}

/// What a publish or prune says once the owner interrupted it.
fn stopping(command: StoreCommand, number: u32) -> String {
    match command {
        StoreCommand::Publish => format!(
            "the publish of release {number} stops once the unit in hand is uploaded and \
             checked, latest left where it was; publishing release {number} again continues \
             from its journal. Ctrl-C again stops at once, and the unit in hand is uploaded \
             again by the next run"
        ),
        StoreCommand::Prune => format!(
            "the prune of release {number} stops once the step in hand is done; pruning \
             release {number} again finishes it. Ctrl-C again stops at once"
        ),
    }
}

#[derive(Args)]
struct BuildArgs {
    #[command(flatten)]
    matching: MatchArgs,
    /// The build output: each release is written to `releases/{n}` under it.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_BUILD)]
    out: PathBuf,
}

#[derive(Args)]
struct TilesArgs {
    #[command(flatten)]
    matching: MatchArgs,
    /// The directory each scale domain's tiles and manifest are written under.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_TILES)]
    out: PathBuf,
}

#[derive(Args)]
struct MatchArgs {
    #[command(flatten)]
    fetch: FetchArgs,
    /// The state directory: the id ledger and the last release's entry ids, in YAML.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_STATE)]
    state: PathBuf,
}

/// How far a run goes, with what the stages after normalization take.
enum Until {
    Fetch,
    Normalize,
    /// A cross-match, its ids pending in the state directory.
    Match {
        state: PathBuf,
    },
    /// A cross-match and its tiles, written under `out`.
    Tiles {
        state: PathBuf,
        out: PathBuf,
    },
    /// A cross-match, its tiles and the release built under `out`.
    Build {
        state: PathBuf,
        out: PathBuf,
    },
}

impl Until {
    /// The state directory a cross-match reads, and holds the lock of while it runs.
    fn state(&self) -> Option<&PathBuf> {
        match self {
            Self::Fetch | Self::Normalize => None,
            Self::Match { state } | Self::Tiles { state, .. } | Self::Build { state, .. } => {
                Some(state)
            }
        }
    }
}

#[derive(Args)]
struct FetchArgs {
    /// Take only this source; repeat for more. All sources when left out.
    #[arg(long = "source", value_name = "ID")]
    sources: Vec<String>,
    /// Make no request: use only what is on disk, and fail on the first item missing.
    #[arg(long)]
    offline: bool,
    /// Where chunks, files and provenance are kept between runs: by default the Univ home
    /// folder's downloads, beside this repository, which its projects share.
    #[arg(long, env = "UNIV_INGEST_CACHE", default_value = DEFAULT_CACHE)]
    cache: PathBuf,
    /// The directory of source definitions.
    #[arg(long, default_value = "sources")]
    definitions: PathBuf,
    /// The repository root, for sources kept in the repository.
    #[arg(long, default_value = ".")]
    repository: PathBuf,
}

/// The stages after normalization, which a run asks for by its command.
struct Later<'a> {
    definitions: &'a [SourceDefinition],
    acquired: &'a [Option<Acquired>],
    options: &'a FetchOptions,
    report: &'a Mutex<RunReport>,
    /// The run's one CMB dipole correction, the normalize stage's.
    correction: CmbCorrection,
    /// Set when the owner interrupts the run: each stage stops at its next safe point.
    cancel: &'a Arc<AtomicBool>,
}

impl Later<'_> {
    /// Cross-matches the `normalized` entries, then lays out the tiles and builds the release
    /// as far as `until` asks.
    async fn run(&self, sink: Arc<dyn EventSink>, normalized: Vec<Normalized>, until: Until) {
        let (defs, opts, report, cancel) =
            (self.definitions, self.options, self.report, self.cancel);
        let Some(state) = until.state().cloned() else {
            return;
        };
        let match_options = MatchOptions {
            state: state.clone(),
            correction: self.correction,
        };
        let crossed = match_stage_until(
            sink.clone(),
            defs,
            self.acquired,
            normalized,
            opts,
            &match_options,
            report,
            cancel,
        )
        .await;
        let Some(crossed) = crossed else {
            return;
        };
        let crossed = Arc::new(crossed);
        // A build keeps the tiles for its release; the tiles command writes them under its
        // output and ends there.
        let (tile_options, build) = match until {
            Until::Fetch | Until::Normalize | Until::Match { .. } => return,
            Until::Tiles { out, .. } => (TileOptions { out: Some(out) }, None),
            Until::Build { state, out } => {
                (TileOptions { out: None }, Some(BuildOptions { state, out }))
            }
        };
        let tiles = tiles_stage(
            sink.clone(),
            defs,
            crossed.clone(),
            &tile_options,
            report,
            cancel,
        )
        .await;
        if let (Some(tiles), Some(build_options)) = (tiles, build) {
            build_stage(
                sink,
                defs,
                self.acquired,
                crossed,
                tiles,
                opts,
                &build_options,
                report,
                cancel,
            )
            .await;
        }
    }
}

/// The state directory's lock for a run that holds one, or none; a lock another run holds is
/// reported and refused.
fn lock_state(until: &Until) -> Result<Option<StateLock>, ()> {
    let Some(dir) = until.state() else {
        return Ok(None);
    };
    StateLock::take(dir, std::process::id(), &now())
        .map(Some)
        .map_err(|error| eprintln!("univ-ingest: {error}"))
}

/// Runs every stage `until` asks for: the fetch on this thread, dropped once `cancel` is set,
/// and the stages after it, each stopping at its next safe point once it is.
async fn every_stage(
    fetcher: &Fetcher<SystemClock>,
    definitions: &[SourceDefinition],
    options: &FetchOptions,
    report: &Mutex<RunReport>,
    cancel: &Arc<AtomicBool>,
    display: &Arc<dyn RunDisplay>,
    until: Until,
) {
    // The fetch runs on this thread: once the owner interrupts the run, it is dropped, and
    // every chunk and partial download stays on disk.
    let acquired = tokio::select! {
        acquired = fetch(fetcher, definitions, options, report) => acquired,
        () = until_set(cancel) => return,
    };
    if matches!(until, Until::Fetch) {
        return;
    }
    let sink = display.as_ref() as &dyn EventSink;
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let normalized = normalize_stage_until(
        sink,
        definitions,
        &acquired,
        options,
        report,
        &correction,
        cancel,
    )
    .await;
    if until.state().is_none() {
        return;
    }
    let stages = Later {
        definitions,
        acquired: &acquired,
        options,
        report,
        correction,
        cancel,
    };
    stages.run(display.clone(), normalized, until).await;
}

/// With the `dhat-heap` feature, every allocation goes through DHAT, which writes the heap's
/// profile to `dhat-heap.json` when the run ends.
/// The download cache, from the repository root, where Univ Ingest runs: the Univ home
/// folder's `downloads/univ-ingest`.
const DEFAULT_CACHE: &str = "../downloads/univ-ingest";

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// mimalloc, except under the heap profiler: it gives back the pages of freed small blocks,
/// which the system allocator keeps.
#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> ExitCode {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let (args, until) = match Cli::parse().command {
        Command::Fetch(args) => (args, Until::Fetch),
        Command::Normalize(args) => (args, Until::Normalize),
        Command::Match(args) => (args.fetch, Until::Match { state: args.state }),
        Command::Tiles(args) => {
            let m = args.matching;
            let (state, out) = (m.state, args.out);
            (m.fetch, Until::Tiles { state, out })
        }
        Command::Build(args) => {
            let m = args.matching;
            let (state, out) = (m.state, args.out);
            (m.fetch, Until::Build { state, out })
        }
        Command::Publish(args) => return on_store(args, StoreCommand::Publish).await,
        Command::Prune(args) => return on_store(args, StoreCommand::Prune).await,
    };
    // A match, tiles or build run holds the state directory's lock until it ends.
    let Ok(_lock) = lock_state(&until) else {
        return ExitCode::FAILURE;
    };
    let options = FetchOptions {
        definitions: args.definitions,
        repository: args.repository,
        cache: args.cache,
        sources: args.sources,
    };
    let definitions = match select(&options) {
        Ok(definitions) => definitions,
        Err(error) => {
            eprintln!("univ-ingest: {error}");
            return ExitCode::FAILURE;
        }
    };
    let display = display_for_stderr();
    let fetcher = match Fetcher::new(
        SystemClock,
        display.clone(),
        RetryPolicy::default(),
        args.offline,
    ) {
        Ok(fetcher) => fetcher,
        Err(error) => {
            eprintln!("univ-ingest: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = Mutex::new(RunReport::new(&definitions));
    let cancel = Arc::new(AtomicBool::new(false));
    let run = every_stage(
        &fetcher,
        &definitions,
        &options,
        &report,
        &cancel,
        &display,
        until,
    );
    let mut run = std::pin::pin!(drawing(display.as_ref(), run));
    let interrupted_run = tokio::select! {
        () = &mut run => false,
        () = owner_interrupt(tokio::signal::ctrl_c()) => true,
    };
    if interrupted_run {
        // Every stage after the fetch runs on a blocking thread, which dropping the run would
        // leave running, and which the runtime would wait for: ask each to stop at its next safe
        // point, and wait until it has, so that the summary says what was really done and the
        // state directory's lock is released only once nothing writes to it.
        cancel.store(true, Ordering::Relaxed);
        let stage = report.lock().map_or(Stage::Fetch, |r| r.stage());
        display.emit(Event::Stopping {
            what: format!(
                "the {} stage stops at its next safe point, and keeps nothing half made",
                stage_name(stage)
            ),
        });
        run.await;
    }
    let report = report.lock().unwrap_or_else(PoisonError::into_inner);
    if interrupted_run {
        let (sources, outcome) = report.interrupted();
        display.finish(sources, outcome);
        eprintln!("{}", interrupted(&options, report.built()));
        return ExitCode::from(INTERRUPTED);
    }
    let (sources, outcome) = report.ended();
    display.finish(sources, outcome);
    // 1, the failure status, when a stage failed; 0 otherwise.
    ExitCode::from(u8::from(report.failed()))
}
