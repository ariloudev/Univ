//! The cross-match stage of a run: the entries every source was normalized into, resolved into
//! the objects of the next release, with the state that release would leave written as pending.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use univ_model::derive::Release;

use super::{FetchOptions, RunReport, lock};
use crate::acquire::Acquired;
use crate::crossmatch::{CrossMatched, Inputs, STEPS, cross_match};
use crate::events::{Event, EventSink, Stage};
use crate::matching::MatchingRules;
use crate::normalize::Normalized;
use crate::ranking::RankingRules;
use crate::source::{Access, SourceDefinition};
use crate::state::State;
use crate::structures::{Structure, read_all};

/// What a cross-match is asked to do beyond a fetch.
#[derive(Clone, Debug)]
pub struct MatchOptions {
    /// The state directory: the id ledger and the last release's entry ids.
    pub state: PathBuf,
}

/// What the cross-match reads besides the entries.
struct Read {
    matching: MatchingRules,
    ranking: RankingRules,
    state: State,
    structures: Vec<Structure>,
}

/// The rules in the repository's `rules/`, the state in `state`, and the researched structures
/// of the repository source among `definitions` that the fetch found.
fn read(
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    options: &FetchOptions,
    state: &Path,
) -> Result<Read, String> {
    let rules = options.repository.join("rules");
    let matching = MatchingRules::read(&rules.join("matching.yaml")).map_err(|e| e.to_string())?;
    let ranking = RankingRules::read(&rules.join("ranking.yaml")).map_err(|e| e.to_string())?;
    let state = State::read(state).map_err(|e| e.to_string())?;
    let mut structures = Vec::new();
    for (definition, on_disk) in definitions.iter().zip(acquired) {
        if let (Access::Repository { path }, Some(_)) = (&definition.access, on_disk) {
            let dir = options.repository.join(path);
            structures.extend(read_all(&dir).map_err(|e| e.to_string())?);
        }
    }
    Ok(Read {
        matching,
        ranking,
        state,
        structures,
    })
}

/// Cross-matches the entries of `normalized` under the repository's rules, the release
/// cosmology tabulated once, the state in `match_options.state` and the researched structures,
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
    lock(report).stage = Stage::Match;
    sink.emit(Event::StageStarted {
        stage: Stage::Match,
        items: Some(STEPS),
    });
    let dir = match_options.state.clone();
    let outcome = match read(definitions, acquired, options, &dir) {
        Err(error) => Err(error),
        Ok(inputs) => {
            let events = sink.clone();
            let entries = normalized.into_iter().flat_map(|n| n.entries).collect();
            tokio::task::spawn_blocking(move || {
                let release = Release::planck_2018();
                let inputs = Inputs {
                    matching: &inputs.matching,
                    ranking: &inputs.ranking,
                    release: &release,
                    state: &inputs.state,
                    structures: &inputs.structures,
                };
                let crossed =
                    cross_match(entries, &inputs, events.as_ref()).map_err(|e| e.to_string())?;
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
        Ok(crossed) => Some(crossed),
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
