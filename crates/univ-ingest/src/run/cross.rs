//! The cross-match stage of a run: the entries every source was normalized into, resolved into
//! the objects of the next release, with the state that release would leave written as pending.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use univ_model::derive::Release;
use univ_model::redshift::CmbCorrection;

use super::{FetchOptions, RunReport, lock};
use crate::acquire::Acquired;
use crate::crossmatch::{Basis, CrossMatched, Inputs, STEPS, cross_match_watched};
use crate::events::{Event, EventSink, Stage};
use crate::normalize::Normalized;
use crate::source::{Access, SourceDefinition};
use crate::state::State;
use crate::watch::Watch;

/// What a cross-match is asked to do beyond a fetch.
#[derive(Clone, Debug)]
pub struct MatchOptions {
    /// The state directory: the id ledger and the last release's entry ids.
    pub state: PathBuf,
    /// The run's one CMB dipole correction, the one the normalize stage used.
    pub correction: CmbCorrection,
}

/// The rules and release configuration in the repository's `rules/`, each read once, and the
/// researched structures of the repository sources among `definitions` that the fetch found.
fn basis(
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    options: &FetchOptions,
) -> Result<Basis, String> {
    let dirs: Vec<PathBuf> = definitions
        .iter()
        .zip(acquired)
        .filter_map(|(d, on_disk)| match (&d.access, on_disk) {
            (Access::Repository { path }, Some(_)) => Some(options.repository.join(path)),
            _ => None,
        })
        .collect();
    Basis::read(&options.repository.join("rules"), &dirs)
}

/// Cross-matches the entries of `normalized` under the repository's rules, the release
/// cosmology of its release configuration tabulated once, the state in `match_options.state` and the researched structures,
/// and writes the state it would leave as pending. A stage that fails is reported in `report`,
/// and gives nothing.
pub async fn match_stage(
    sink: Arc<dyn EventSink>,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    normalized: Vec<Normalized>,
    options: &FetchOptions,
    match_options: &MatchOptions,
    report: &Mutex<RunReport>,
) -> Option<CrossMatched> {
    let never = Arc::new(AtomicBool::new(false));
    match_stage_until(
        sink,
        definitions,
        acquired,
        normalized,
        options,
        match_options,
        report,
        &never,
    )
    .await
}

/// [`match_stage`], stopping after the step in hand once `cancel` is set: it then gives nothing,
/// writes no pending state, and reports no failure, as the run was interrupted.
#[expect(
    clippy::too_many_arguments,
    reason = "a stage takes the run's sink, sources, inputs, options, report and cancel flag"
)]
pub async fn match_stage_until(
    sink: Arc<dyn EventSink>,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    normalized: Vec<Normalized>,
    options: &FetchOptions,
    match_options: &MatchOptions,
    report: &Mutex<RunReport>,
    cancel: &Arc<AtomicBool>,
) -> Option<CrossMatched> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    lock(report).stage = Stage::Match;
    sink.emit(Event::StageStarted {
        stage: Stage::Match,
        items: Some(STEPS),
    });
    let dir = match_options.state.clone();
    let read = basis(definitions, acquired, options)
        .and_then(|b| State::read(&dir).map(|s| (b, s)).map_err(|e| e.to_string()));
    let outcome = match read {
        Err(error) => Err(error),
        Ok((basis, state)) => {
            let (events, flag) = (sink.clone(), cancel.clone());
            let correction = match_options.correction;
            // Gathered into one vector of its final size, each source's let go as it is moved.
            let mut entries = Vec::with_capacity(normalized.iter().map(|n| n.entries.len()).sum());
            for n in normalized {
                entries.extend(n.entries);
            }
            tokio::task::spawn_blocking(move || {
                // The ranking and the derived redshift distances share one peculiar velocity.
                let release = Release::new(&basis.config.cosmology)
                    .with_peculiar_velocity(basis.ranking.peculiar_velocity_km_s);
                let inputs = Inputs {
                    basis: &basis,
                    release: &release,
                    correction: &correction,
                    state: &state,
                };
                let watch = Watch::new(&flag, &|_| {});
                let crossed = cross_match_watched(entries, &inputs, events.as_ref(), watch)
                    .map_err(|e| e.to_string())?;
                watch.check()?;
                crossed
                    .next
                    .write_pending(&dir)
                    .map_err(|e| format!("the pending state cannot be written: {e}"))?;
                Ok(crossed)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
        }
    };
    match outcome {
        Ok(crossed) => {
            super::used::mark(report, definitions, acquired, super::Reached::Match);
            Some(crossed)
        }
        Err(_) if cancel.load(Ordering::Relaxed) => None,
        Err(error) => {
            lock(report).failure = Some(error);
            sink.emit(Event::StageEnded {
                stage: Stage::Match,
                counts: vec![],
            });
            None
        }
    }
}
