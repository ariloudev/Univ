//! The summary a run prints when it ends.

use std::time::Duration;

use univ_model::ids::SourceId;

use super::{clock, counted, rate, size, stage_name};
use crate::events::{Count, Stage};

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run built this release.
    Built(u64),
    /// The run published this release.
    Published(u64),
    /// The run pruned this release.
    Pruned(u64),
    /// The run migrated the store, or found it needed nothing.
    Migrated(bool),
    /// The run stopped at a stage, for a reason.
    Stopped { stage: Stage, reason: String },
}

/// How far a run took one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceState {
    /// The source is on disk, whole.
    Fetched,
    /// The source could not be fetched, for this reason.
    Failed(String),
    /// The source is on disk and is no table of rows, so it is not normalized; the run used it
    /// as this says, such as `volume grid, written to release 6`.
    Used(String),
    /// The run ended before the source did.
    NotReached,
}

/// What a run did with one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSummary {
    pub source: SourceId,
    pub state: SourceState,
    /// Catalogue entries published and left out, once the source has been normalized.
    pub published: Option<u64>,
    pub left_out: Option<u64>,
}

/// What a stage that ended counted, such as the objects of a cross-match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageCounts {
    pub stage: Stage,
    pub counts: Vec<Count>,
}

/// What a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub bytes: u64,
    pub elapsed: Duration,
    pub sources: Vec<SourceSummary>,
    /// The counts of each stage that ended with some, in order.
    pub stages: Vec<StageCounts>,
    pub outcome: Outcome,
}

impl Summary {
    /// The summary's lines.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let elapsed = clock(self.elapsed.as_secs());
        let moved = |what: &str| {
            format!(
                "{what} {} in {elapsed} ({}/s)",
                size(self.bytes),
                size(rate(self.bytes, self.elapsed))
            )
        };
        let mut lines = vec![match self.outcome {
            Outcome::Published(_)
            | Outcome::Stopped {
                stage: Stage::Publish,
                ..
            } => moved("Uploaded"),
            Outcome::Pruned(_)
            | Outcome::Migrated(_)
            | Outcome::Stopped {
                stage: Stage::Prune | Stage::Migrate,
                ..
            } => format!("Ran for {elapsed}"),
            _ => moved("Fetched"),
        }];
        for s in &self.sources {
            let what = match (&s.state, s.published, s.left_out) {
                (SourceState::Failed(reason), ..) => format!("failed: {reason}"),
                (SourceState::NotReached, ..) => "not reached in this run".to_owned(),
                (SourceState::Used(how), ..) => how.clone(),
                (SourceState::Fetched, Some(p), Some(l)) => {
                    format!("{p} entries published, {l} left out")
                }
                (SourceState::Fetched, ..) => "not normalized in this run".to_owned(),
            };
            lines.push(format!("  {}: {what}", s.source));
        }
        for s in &self.stages {
            let counts = counted(&s.counts);
            lines.push(format!("  {}{counts}", stage_name(s.stage)));
        }
        lines.push(match &self.outcome {
            Outcome::Built(n) => format!("Built release {n}"),
            Outcome::Published(n) => format!("Published release {n}"),
            Outcome::Pruned(n) => format!("Pruned release {n}"),
            Outcome::Migrated(true) => "Migrated the store to shared rows".to_owned(),
            Outcome::Migrated(false) => "The store needed no migration".to_owned(),
            Outcome::Stopped { stage, reason } => {
                format!("Stopped at {}: {reason}", stage_name(*stage))
            }
        });
        lines
    }
}
