//! A run of Univ Ingest: which display it uses, and the fetch of every source it is asked for.

use std::io::{IsTerminal as _, Write};
use std::path::PathBuf;
use std::time::Instant;

use crate::access::{chunk_requests, count_request};
use crate::acquire::acquire;
use crate::display::{IndicatifTarget, Outcome, PlainSink, SourceSummary, Summary, TerminalSink};
use crate::events::{Event, EventSink, Stage};
use crate::fetch::{Clock, Fetcher, SystemClock};
use crate::source::{Access, SourceDefinition, read_all};

/// What a fetch is asked to do.
#[derive(Clone, Debug)]
pub struct FetchOptions {
    /// The directory of source definitions.
    pub definitions: PathBuf,
    /// The repository root, against which repository sources are read.
    pub repository: PathBuf,
    /// Where chunks, files and provenance are kept between runs.
    pub cache: PathBuf,
    /// The ids of the sources to fetch; all of them when empty.
    pub sources: Vec<String>,
}

/// The display of a run: the terminal one on a terminal, plain lines elsewhere.
pub enum Display {
    Terminal(TerminalSink<SystemClock, IndicatifTarget>),
    Plain(PlainSink<SystemClock, Box<dyn Write + Send>>),
}

impl Display {
    /// The terminal display when standard error is a terminal, plain lines otherwise.
    #[must_use]
    pub fn for_stderr() -> Self {
        if std::io::stderr().is_terminal() {
            Self::Terminal(TerminalSink::new(SystemClock, IndicatifTarget::stderr()))
        } else {
            Self::Plain(PlainSink::new(SystemClock, Box::new(std::io::stderr())))
        }
    }

    /// Prints the summary, after removing the status line on a terminal.
    pub fn finish(&self, summary: &Summary) {
        match self {
            Self::Terminal(sink) => sink.finish(summary),
            Self::Plain(sink) => sink.finish(summary),
        }
    }

    /// The bytes fetched so far.
    #[must_use]
    pub fn fetched(&self) -> u64 {
        match self {
            Self::Terminal(sink) => sink.fetched(),
            Self::Plain(sink) => sink.fetched(),
        }
    }
}

impl EventSink for Display {
    fn emit(&self, event: Event) {
        match self {
            Self::Terminal(sink) => sink.emit(event),
            Self::Plain(sink) => sink.emit(event),
        }
    }
}

/// What a fetch came to: the sources it fetched, and the failures it met.
#[derive(Debug, Default)]
pub struct FetchReport {
    pub fetched: Vec<SourceSummary>,
    pub failures: Vec<String>,
}

/// The definitions a run is asked for.
///
/// # Errors
///
/// Fails when a definition cannot be used or an id names no definition.
pub fn select(options: &FetchOptions) -> Result<Vec<SourceDefinition>, String> {
    let all = read_all(&options.definitions).map_err(|e| e.to_string())?;
    if options.sources.is_empty() {
        return Ok(all);
    }
    options
        .sources
        .iter()
        .map(|id| {
            all.iter()
                .find(|d| d.id.as_str() == id)
                .cloned()
                .ok_or_else(|| format!("no source is defined with the id {id}"))
        })
        .collect()
}

/// The chunks, count queries and files the run will fetch.
fn items(definitions: &[SourceDefinition]) -> u64 {
    definitions
        .iter()
        .map(|d| match &d.access {
            Access::Files { files } => files.len(),
            Access::Repository { .. } => 0,
            _ => chunk_requests(d).len() + usize::from(count_request(d).is_some()),
        })
        .map(|n| n as u64)
        .sum()
}

/// Fetches every definition into the cache. A source that fails is reported and left out, and
/// the others are still fetched.
pub async fn fetch<C: Clock>(
    fetcher: &Fetcher<C>,
    sink: &dyn EventSink,
    definitions: &[SourceDefinition],
    options: &FetchOptions,
) -> FetchReport {
    sink.emit(Event::StageStarted {
        stage: Stage::Fetch,
        items: Some(items(definitions)),
    });
    let mut report = FetchReport::default();
    for definition in definitions {
        match acquire(fetcher, definition, &options.cache, &options.repository).await {
            Ok(_) => report.fetched.push(SourceSummary {
                source: definition.id.clone(),
                published: None,
                left_out: None,
            }),
            Err(error) => report.failures.push(error.to_string()),
        }
    }
    sink.emit(Event::StageEnded {
        stage: Stage::Fetch,
    });
    report
}

/// The summary of a fetch-only run.
#[must_use]
pub fn summary(report: FetchReport, bytes: u64, started: Instant) -> Summary {
    let reason = if report.failures.is_empty() {
        "fetch only; no release was asked for".to_owned()
    } else {
        format!(
            "{} sources failed: {}",
            report.failures.len(),
            report.failures.join("; ")
        )
    };
    Summary {
        bytes,
        elapsed: started.elapsed(),
        sources: report.fetched,
        outcome: Outcome::Stopped {
            stage: Stage::Fetch,
            reason,
        },
    }
}

/// What an interrupted run says: where its work is kept, and how to continue.
#[must_use]
pub fn interrupted(options: &FetchOptions) -> String {
    format!(
        "Interrupted. Every chunk and partial download is kept in {}; run the same command again \
         to continue from where this run stopped. No release was built.",
        options.cache.display()
    )
}
