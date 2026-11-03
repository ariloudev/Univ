//! A run of Univ Ingest: which display it uses, and the fetch of every source it is asked for.

use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::time::MissedTickBehavior;

use crate::access::{chunk_requests, count_request};
use crate::acquire::acquire;
use crate::display::{
    IndicatifTarget, Outcome, PlainSink, REDRAW_EVERY, RunDisplay, SourceState, SourceSummary,
    TerminalSink,
};
use crate::events::{Event, Stage};
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

/// The display of a run: the terminal one when standard error is a terminal, plain lines
/// otherwise.
#[must_use]
pub fn display_for_stderr() -> Arc<dyn RunDisplay> {
    if std::io::stderr().is_terminal() {
        Arc::new(TerminalSink::new(SystemClock, IndicatifTarget::stderr()))
    } else {
        Arc::new(PlainSink::new(SystemClock, std::io::stderr()))
    }
}

/// Runs `work`, drawing `display` every [`REDRAW_EVERY`] until it ends.
pub async fn drawing<T>(display: &dyn RunDisplay, work: impl Future<Output = T>) -> T {
    let mut ticks = tokio::time::interval(REDRAW_EVERY);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut work = std::pin::pin!(work);
    loop {
        tokio::select! {
            out = &mut work => return out,
            _ = ticks.tick() => display.draw(),
        }
    }
}

/// Resolves when the owner presses Ctrl-C, which `signal` waits for. When Ctrl-C cannot be
/// caught, it warns and never resolves, so the run goes on rather than being taken as
/// interrupted.
pub async fn owner_interrupt(signal: impl Future<Output = std::io::Result<()>>) {
    if let Err(error) = signal.await {
        eprintln!(
            "univ-ingest: warning: Ctrl-C cannot be caught ({error}), so an interrupted run \
             prints no summary"
        );
        std::future::pending::<()>().await;
    }
}

/// How far a fetch has taken each source it was asked for, updated as each source ends, so an
/// interrupted run can still say.
#[derive(Debug)]
pub struct FetchReport {
    sources: Vec<SourceSummary>,
}

impl FetchReport {
    /// The report of a fetch of `definitions` that has not reached any of them yet.
    #[must_use]
    pub fn new(definitions: &[SourceDefinition]) -> Self {
        let sources = definitions
            .iter()
            .map(|d| SourceSummary {
                source: d.id.clone(),
                state: SourceState::NotReached,
                published: None,
                left_out: None,
            })
            .collect();
        Self { sources }
    }

    /// Each source and how far the fetch took it.
    #[must_use]
    pub fn sources(&self) -> &[SourceSummary] {
        &self.sources
    }

    /// Whether some source failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.sources
            .iter()
            .any(|s| matches!(s.state, SourceState::Failed(_)))
    }

    /// The sources and outcome of a fetch that ran to its end.
    #[must_use]
    pub fn ended(&self) -> (Vec<SourceSummary>, Outcome) {
        let failed = self
            .sources
            .iter()
            .filter(|s| matches!(s.state, SourceState::Failed(_)))
            .count();
        let reason = if failed == 0 {
            "fetch only; no release was asked for".to_owned()
        } else {
            format!("{failed} of {} sources failed", self.sources.len())
        };
        let outcome = Outcome::Stopped {
            stage: Stage::Fetch,
            reason,
        };
        (self.sources.clone(), outcome)
    }

    /// The sources and outcome of a fetch the owner interrupted: the sources it had not ended
    /// stay not reached.
    #[must_use]
    pub fn interrupted(&self) -> (Vec<SourceSummary>, Outcome) {
        let outcome = Outcome::Stopped {
            stage: Stage::Fetch,
            reason: "interrupted by the owner".to_owned(),
        };
        (self.sources.clone(), outcome)
    }
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

/// Fetches every definition into the cache, marking each source in `report` as it ends. A
/// source that fails is reported and left out, and the others are still fetched.
pub async fn fetch<C: Clock>(
    fetcher: &Fetcher<C>,
    definitions: &[SourceDefinition],
    options: &FetchOptions,
    report: &Mutex<FetchReport>,
) {
    fetcher.emit(Event::StageStarted {
        stage: Stage::Fetch,
        items: Some(items(definitions)),
    });
    for (n, definition) in definitions.iter().enumerate() {
        let state = match acquire(fetcher, definition, &options.cache, &options.repository).await {
            Ok(_) => SourceState::Fetched,
            Err(error) => {
                let error = error.to_string();
                // The summary line names the source already.
                let prefix = format!("{}: ", definition.id);
                SourceState::Failed(error.strip_prefix(&prefix).unwrap_or(&error).to_owned())
            }
        };
        let mut report = report.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(source) = report.sources.get_mut(n) {
            source.state = state;
        }
    }
    fetcher.emit(Event::StageEnded {
        stage: Stage::Fetch,
    });
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
