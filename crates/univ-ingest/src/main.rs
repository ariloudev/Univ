//! Univ Ingest: the command-line tool that downloads public sky data, normalizes it, resolves
//! catalogue entries into objects and publishes the result as a release that Univ Server serves.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use clap::{Args, Parser, Subcommand};
use univ_ingest::acquire::Acquired;
use univ_ingest::events::{EventSink, Stage};
use univ_ingest::fetch::{Fetcher, RetryPolicy, SystemClock};
use univ_ingest::normalize::Normalized;
use univ_ingest::run::{
    BuildOptions, DEFAULT_BUILD, DEFAULT_CONFIG, DEFAULT_PERSIST, DEFAULT_TILES, FetchOptions,
    MatchOptions, RunReport, StoreOptions, TileOptions, build_stage, display_for_stderr, drawing,
    fetch, interrupted, match_stage, normalize_stage_until, now, owner_interrupt, prune_command,
    publish_command, select, tiles_stage,
};
use univ_ingest::source::SourceDefinition;
use univ_ingest::state::DEFAULT_STATE;

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
    /// pruning time and keeping the id ledger, so every id still resolves. The release latest
    /// points to is refused.
    Prune(StoreArgs),
}

#[derive(Args)]
struct StoreArgs {
    /// The release number.
    number: u32,
    /// The build output, whose `releases/{n}` holds the release.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_BUILD)]
    out: PathBuf,
    /// Wrangler's local state directory, as `wrangler dev --persist-to` takes it.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_PERSIST)]
    persist_to: PathBuf,
    /// The Wrangler configuration naming the local D1 and R2 bindings; Wrangler is run from
    /// the `node_modules` beside it.
    #[arg(long, value_name = "FILE", default_value = DEFAULT_CONFIG)]
    config: PathBuf,
}

/// Runs the publish or prune command, a plain, timestamped line per step on standard error.
fn on_store(args: StoreArgs, pruning: bool) -> ExitCode {
    let options = StoreOptions {
        number: args.number,
        out: args.out,
        persist_to: args.persist_to,
        config: args.config,
    };
    let mut line = |text: String| eprintln!("{} {text}", now());
    let outcome = if pruning {
        prune_command(&options, &mut line)
    } else {
        publish_command(&options, &mut line)
    };
    match outcome {
        Ok(closing) => {
            eprintln!("{} {closing}", now());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{} univ-ingest: {error}", now());
            ExitCode::FAILURE
        }
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

/// How far a run goes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Until {
    Fetch,
    Normalize,
    Match,
    Tiles,
    Build,
}

#[derive(Args)]
struct FetchArgs {
    /// Take only this source; repeat for more. All sources when left out.
    #[arg(long = "source", value_name = "ID")]
    sources: Vec<String>,
    /// Make no request: use only what is on disk, and fail on the first item missing.
    #[arg(long)]
    offline: bool,
    /// Where chunks, files and provenance are kept between runs.
    #[arg(long, default_value = ".cache/univ-ingest")]
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
}

impl Later<'_> {
    /// Cross-matches the `normalized` entries, then lays out the tiles and builds the release
    /// as far as `until` asks.
    async fn run(
        &self,
        sink: Arc<dyn EventSink>,
        normalized: Vec<Normalized>,
        state: PathBuf,
        out: Option<PathBuf>,
        until: Until,
    ) {
        let (defs, opts, report) = (self.definitions, self.options, self.report);
        let match_options = MatchOptions {
            state: state.clone(),
        };
        let crossed = match_stage(
            sink.clone(),
            defs,
            self.acquired,
            normalized,
            opts,
            &match_options,
            report,
        )
        .await;
        let (Some(crossed), Some(out)) = (crossed, out) else {
            return;
        };
        let crossed = Arc::new(crossed);
        let building = until == Until::Build;
        // A build keeps the tiles for its release; the tiles command writes them under its
        // output and ends there.
        let tile_options = TileOptions {
            out: (!building).then(|| out.clone()),
        };
        let tiles = tiles_stage(
            sink.clone(),
            defs,
            crossed.clone(),
            opts,
            &tile_options,
            report,
        )
        .await;
        if let (Some(tiles), true) = (tiles, building) {
            let build_options = BuildOptions { state, out };
            build_stage(
                sink,
                defs,
                self.acquired,
                crossed,
                tiles,
                opts,
                &build_options,
                report,
            )
            .await;
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let (args, until, state, out) = match Cli::parse().command {
        Command::Fetch(args) => (args, Until::Fetch, None, None),
        Command::Normalize(args) => (args, Until::Normalize, None, None),
        Command::Match(args) => (args.fetch, Until::Match, Some(args.state), None),
        Command::Tiles(args) => {
            let m = args.matching;
            (m.fetch, Until::Tiles, Some(m.state), Some(args.out))
        }
        Command::Build(args) => {
            let m = args.matching;
            (m.fetch, Until::Build, Some(m.state), Some(args.out))
        }
        Command::Publish(args) => return on_store(args, false),
        Command::Prune(args) => return on_store(args, true),
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
    let run = async {
        let acquired = fetch(&fetcher, &definitions, &options, &report).await;
        if until < Until::Normalize {
            return;
        }
        let sink = display.as_ref() as &dyn EventSink;
        let normalized =
            normalize_stage_until(sink, &definitions, &acquired, &options, &report, &cancel).await;
        let Some(state) = state else {
            return;
        };
        let stages = Later {
            definitions: &definitions,
            acquired: &acquired,
            options: &options,
            report: &report,
        };
        stages
            .run(display.clone(), normalized, state, out, until)
            .await;
    };
    let mut run = std::pin::pin!(drawing(display.as_ref(), run));
    let interrupted_run = tokio::select! {
        () = &mut run => false,
        () = owner_interrupt(tokio::signal::ctrl_c()) => true,
    };
    let normalizing_now = report.lock().is_ok_and(|r| r.stage() == Stage::Normalize);
    if interrupted_run && normalizing_now {
        // A source is normalized on a blocking thread, which dropping the run would leave
        // running: ask it to stop, and wait until it has.
        cancel.store(true, Ordering::Relaxed);
        run.await;
    }
    let report = report.lock().unwrap_or_else(PoisonError::into_inner);
    if interrupted_run {
        let (sources, outcome) = report.interrupted();
        display.finish(sources, outcome);
        eprintln!("{}", interrupted(&options));
        return ExitCode::from(INTERRUPTED);
    }
    let (sources, outcome) = report.ended();
    display.finish(sources, outcome);
    if report.failed() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
