//! The summary a run prints when it ends.

use std::time::Duration;

use univ_model::ids::SourceId;

use super::{clock, rate, size, stage_name};
use crate::events::Stage;

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run built this release.
    Built(u64),
    /// The run published this release.
    Published(u64),
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

/// What a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub bytes: u64,
    pub elapsed: Duration,
    pub sources: Vec<SourceSummary>,
    pub outcome: Outcome,
}

impl Summary {
    /// The summary's lines.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Fetched {} in {} ({}/s)",
            size(self.bytes),
            clock(self.elapsed.as_secs()),
            size(rate(self.bytes, self.elapsed))
        )];
        for s in &self.sources {
            let what = match (&s.state, s.published, s.left_out) {
                (SourceState::Failed(reason), ..) => format!("failed: {reason}"),
                (SourceState::NotReached, ..) => "not reached in this run".to_owned(),
                (SourceState::Fetched, Some(p), Some(l)) => {
                    format!("{p} entries published, {l} left out")
                }
                (SourceState::Fetched, ..) => "not normalized in this run".to_owned(),
            };
            lines.push(format!("  {}: {what}", s.source));
        }
        lines.push(match &self.outcome {
            Outcome::Built(n) => format!("Built release {n}"),
            Outcome::Published(n) => format!("Published release {n}"),
            Outcome::Stopped { stage, reason } => {
                format!("Stopped at {}: {reason}", stage_name(*stage))
            }
        });
        lines
    }
}
