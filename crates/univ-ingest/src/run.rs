//! A run of Univ Ingest: which display it uses, the fetch of every source it is asked for, and
//! the normalization of each source fetched.

use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::time::MissedTickBehavior;

use crate::access::item_count;
use univ_model::redshift::{CmbCorrection, DipoleModel};

use crate::acquire::{Acquired, acquire};
use crate::display::{
    IndicatifTarget, Outcome, PlainTarget, REDRAW_EVERY, RunDisplay, RunSink, SourceState,
    SourceSummary, stage_name,
};
use crate::events::{Event, EventSink, Stage};
use crate::fetch::{Clock, Fetcher, SystemClock};
use crate::normalize::{NormalizeError, Normalized, normalize_until, reads_rows};
use crate::provenance::Provenance;
use crate::source::{SourceDefinition, read_all};

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
        Arc::new(RunSink::new(SystemClock, IndicatifTarget::stderr()))
    } else {
        Arc::new(RunSink::new(
            SystemClock,
            PlainTarget::new(std::io::stderr()),
        ))
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

/// How far a run has taken each source it was asked for, updated as each source ends a stage,
/// so an interrupted run can still say.
#[derive(Debug)]
pub struct RunReport {
    sources: Vec<SourceSummary>,
    /// The stage the run is in, or last ended.
    stage: Stage,
}

impl RunReport {
    /// The report of a run of `definitions` that has not reached any of them yet.
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
        Self {
            sources,
            stage: Stage::Fetch,
        }
    }

    /// Each source and how far the run took it.
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

    fn set(&mut self, n: usize, state: SourceState) {
        if let Some(source) = self.sources.get_mut(n) {
            source.state = state;
        }
    }

    /// The sources and outcome of a run that ran to the end of its last stage, which was asked
    /// for without a release.
    #[must_use]
    pub fn ended(&self) -> (Vec<SourceSummary>, Outcome) {
        let failed = self
            .sources
            .iter()
            .filter(|s| matches!(s.state, SourceState::Failed(_)))
            .count();
        let reason = if failed == 0 {
            format!("{} only; no release was asked for", stage_name(self.stage))
        } else {
            format!("{failed} of {} sources failed", self.sources.len())
        };
        let outcome = Outcome::Stopped {
            stage: self.stage,
            reason,
        };
        (self.sources.clone(), outcome)
    }

    /// The stage the run is in.
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// The sources and outcome of a run the owner interrupted: the sources it had not ended
    /// stay as far as they got.
    #[must_use]
    pub fn interrupted(&self) -> (Vec<SourceSummary>, Outcome) {
        let outcome = Outcome::Stopped {
            stage: self.stage,
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
    definitions.iter().map(item_count).sum()
}

/// An error without the source's id it leads with, as the summary line names the source.
fn unprefixed(definition: &SourceDefinition, error: &str) -> String {
    let prefix = format!("{}: ", definition.id);
    error.strip_prefix(&prefix).unwrap_or(error).to_owned()
}

/// Fetches every definition into the cache, marking each source in `report` as it ends, and
/// gives what each source has on disk, or none for a source that failed. A source that fails
/// is reported and left out, and the others are still fetched.
pub async fn fetch<C: Clock>(
    fetcher: &Fetcher<C>,
    definitions: &[SourceDefinition],
    options: &FetchOptions,
    report: &Mutex<RunReport>,
) -> Vec<Option<Acquired>> {
    lock(report).stage = Stage::Fetch;
    fetcher.emit(Event::StageStarted {
        stage: Stage::Fetch,
        items: Some(items(definitions)),
    });
    let mut acquired = Vec::new();
    for (n, definition) in definitions.iter().enumerate() {
        let (state, on_disk) =
            match acquire(fetcher, definition, &options.cache, &options.repository).await {
                Ok(on_disk) => (SourceState::Fetched, Some(on_disk)),
                Err(error) => (
                    SourceState::Failed(unprefixed(definition, &error.to_string())),
                    None,
                ),
            };
        lock(report).set(n, state);
        acquired.push(on_disk);
    }
    fetcher.emit(Event::StageEnded {
        stage: Stage::Fetch,
        counts: vec![],
    });
    acquired
}

/// Normalizes each source that is a table of rows and that the fetch left whole on disk,
/// with one CMB correction for the run, and records what became of its rows in its
/// provenance. It gives each source's entries, kept in memory for the next stage. A source
/// that fails is reported and left out, with no earlier report left in its provenance, and
/// the others are still normalized.
pub async fn normalize_stage(
    sink: &dyn EventSink,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    options: &FetchOptions,
    report: &Mutex<RunReport>,
) -> Vec<Normalized> {
    let cancel = Arc::new(AtomicBool::new(false));
    normalize_stage_until(sink, definitions, acquired, options, report, &cancel).await
}

/// [`normalize_stage`], stopping soon after `cancel` is set: the source being normalized stops
/// at its next look at the flag, keeps no report in its provenance, and no other source starts.
pub async fn normalize_stage_until(
    sink: &dyn EventSink,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    options: &FetchOptions,
    report: &Mutex<RunReport>,
    cancel: &Arc<AtomicBool>,
) -> Vec<Normalized> {
    let tables: Vec<(usize, &SourceDefinition, &Acquired)> = definitions
        .iter()
        .zip(acquired)
        .enumerate()
        .filter_map(|(n, (d, a))| Some((n, d, a.as_ref()?)))
        .filter(|(_, d, _)| reads_rows(d))
        .collect();
    lock(report).stage = Stage::Normalize;
    sink.emit(Event::StageStarted {
        stage: Stage::Normalize,
        items: Some(tables.len() as u64),
    });
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let mut normalized = Vec::new();
    for (n, definition, on_disk) in tables {
        let (owned, files, flag) = (definition.clone(), on_disk.files.clone(), cancel.clone());
        let dir = options.cache.join(definition.id.as_str());
        let outcome = tokio::task::spawn_blocking(move || {
            normalize_until(&owned, &files, &correction, &flag)
        })
        .await;
        let cancelled = matches!(outcome, Ok(Err(NormalizeError::Cancelled { .. })));
        let outcome = outcome
            .map_err(|e| e.to_string())
            .and_then(|outcome| outcome.map_err(|e| e.to_string()))
            .and_then(|done| {
                Provenance::normalized(&dir, &definition.id, &done.report)
                    .map(|()| done)
                    .map_err(|e| format!("its provenance cannot be written: {e}"))
            });
        if outcome.is_err() {
            // The report kept from an earlier run no longer describes what this run made of
            // the source; failing to remove it is part of the failure already reported.
            let _ = Provenance::unnormalized(&dir);
        }
        if cancelled {
            break;
        }
        let source = definition.id.clone();
        match outcome {
            Ok(done) => {
                let (published, left_out) = (done.report.published, done.report.left_out_total());
                sink.emit(Event::Normalized {
                    source,
                    published,
                    left_out,
                });
                let mut report = lock(report);
                if let Some(s) = report.sources.get_mut(n) {
                    (s.published, s.left_out) = (Some(published), Some(left_out));
                }
                normalized.push(done);
            }
            Err(error) => {
                let error = unprefixed(definition, &error);
                sink.emit(Event::NormalizeFailed {
                    source,
                    error: error.clone(),
                });
                lock(report).set(n, SourceState::Failed(error));
            }
        }
    }
    sink.emit(Event::StageEnded {
        stage: Stage::Normalize,
        counts: vec![],
    });
    normalized
}

fn lock(report: &Mutex<RunReport>) -> std::sync::MutexGuard<'_, RunReport> {
    report.lock().unwrap_or_else(PoisonError::into_inner)
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
