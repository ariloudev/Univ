//! Univ Ingest: the command-line tool that downloads public sky data, normalizes it, resolves
//! catalogue entries into objects and publishes the result as a release that Univ Server serves.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Mutex, PoisonError};

use clap::{Args, Parser, Subcommand};
use univ_ingest::fetch::{Fetcher, RetryPolicy, SystemClock};
use univ_ingest::run::{
    FetchOptions, FetchReport, display_for_stderr, drawing, fetch, interrupted, owner_interrupt,
    select,
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
}

#[derive(Args)]
struct FetchArgs {
    /// Fetch only this source; repeat for more. All sources when left out.
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
    let Command::Fetch(args) = Cli::parse().command;
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
    let report = Mutex::new(FetchReport::new(&definitions));
    let fetching = fetch(&fetcher, &definitions, &options, &report);
    let interrupted_run = tokio::select! {
        () = drawing(display.as_ref(), fetching) => false,
        () = owner_interrupt(tokio::signal::ctrl_c()) => true,
    };
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
