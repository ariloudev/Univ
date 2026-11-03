//! Univ Ingest: the command-line tool that downloads public sky data, normalizes it, resolves
//! catalogue entries into objects and publishes the result as a release that Univ Server serves.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use univ_ingest::display::{Outcome, Summary};
use univ_ingest::events::Stage;
use univ_ingest::fetch::{Fetcher, RetryPolicy, SystemClock};
use univ_ingest::run::{Display, FetchOptions, fetch, interrupted, select, summary};

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
    let display = Arc::new(Display::for_stderr());
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
    let started = Instant::now();
    tokio::select! {
        report = fetch(&fetcher, display.as_ref(), &definitions, &options) => {
            let failed = !report.failures.is_empty();
            display.finish(&summary(report, display.fetched(), started));
            if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        _ = tokio::signal::ctrl_c() => {
            display.finish(&Summary {
                bytes: display.fetched(),
                elapsed: started.elapsed(),
                sources: Vec::new(),
                outcome: Outcome::Stopped { stage: Stage::Fetch, reason: "interrupted by the owner".to_owned() },
            });
            eprintln!("{}", interrupted(&options));
            ExitCode::from(INTERRUPTED)
        }
    }
}
