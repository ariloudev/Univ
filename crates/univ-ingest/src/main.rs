//! Univ Ingest: the command-line tool that downloads public sky data, normalizes it, resolves
//! catalogue entries into objects and publishes the result as a release that Univ Server serves.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use clap::{Args, Parser, Subcommand};
use univ_ingest::events::{EventSink, Stage};
use univ_ingest::fetch::{Fetcher, RetryPolicy, SystemClock};
use univ_ingest::run::{
    FetchOptions, RunReport, display_for_stderr, drawing, fetch, interrupted,
    normalize_stage_until, owner_interrupt, select,
};

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

#[tokio::main]
async fn main() -> ExitCode {
    let (args, normalizing) = match Cli::parse().command {
        Command::Fetch(args) => (args, false),
        Command::Normalize(args) => (args, true),
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
        if normalizing {
            // The entries are held for the stages that follow; this command ends here.
            let sink = display.as_ref() as &dyn EventSink;
            normalize_stage_until(sink, &definitions, &acquired, &options, &report, &cancel).await;
        }
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
