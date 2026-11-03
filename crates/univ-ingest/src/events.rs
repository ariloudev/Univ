//! The events of a run, which the run display renders and tests record.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use univ_model::ids::SourceId;

/// A stage of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Fetch,
    Normalize,
    Match,
    Tiles,
    Release,
    Publish,
    Prune,
}

/// A number a stage counted, and what it counts, such as 98 `objects`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Count {
    pub n: u64,
    pub what: String,
}

impl Count {
    #[must_use]
    pub fn new(n: u64, what: impl Into<String>) -> Self {
        Self {
            n,
            what: what.into(),
        }
    }
}

/// Why a partial download started over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartReason {
    /// The provider answered a range request with the whole file.
    RangeIgnored,
    /// The provider's file changed since the partial download began, as its entity tag or
    /// length shows.
    Changed,
    /// The provider could not serve the range asked for.
    RangeUnsatisfiable,
}

/// Something that happened in a run. `item` names a chunk or file of a source.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    StageStarted {
        stage: Stage,
        items: Option<u64>,
    },
    /// A stage ended, with what it counted, such as the objects a cross-match resolved; none
    /// for a stage that reports per source.
    StageEnded {
        stage: Stage,
        counts: Vec<Count>,
    },
    /// A step of a stage is done, such as the ranking of a cross-match; `step` says what it
    /// did.
    Stepped {
        stage: Stage,
        step: String,
    },
    /// A file of a publish was uploaded, `bytes` of it, and checked or imported; `step` says
    /// which and how. It counts as a step of the stage.
    Uploaded {
        stage: Stage,
        step: String,
        bytes: u64,
    },
    /// The owner interrupted the run, which is stopping as `what` says.
    Stopping {
        what: String,
    },
    /// A structure's member list cannot be placed whole: of `members`, `no_object` name no
    /// matched object and `unplaced` name an object that is not placed, so the structure is
    /// shown at its published position, when it has one (`point`).
    MembersUnplaced {
        structure: String,
        members: u64,
        no_object: u64,
        unplaced: u64,
        point: bool,
    },
    /// A request was sent.
    Get {
        source: SourceId,
        item: String,
        url: String,
    },
    /// The item was already on disk, so nothing was fetched.
    Hit {
        source: SourceId,
        item: String,
    },
    /// A request failed; `retry_in` is the wait before the next attempt on the same mirror.
    Err {
        source: SourceId,
        item: String,
        mirror: String,
        attempt: u32,
        error: String,
        retry_in: Option<Duration>,
    },
    /// A mirror failed all its retries and is skipped for the rest of the run.
    MirrorSkipped {
        source: SourceId,
        mirror: String,
    },
    /// Bytes arrived; `total` is the item's size when the provider gives it.
    Received {
        source: SourceId,
        item: String,
        bytes: u64,
        total: Option<u64>,
    },
    /// A partial download continues from byte `from`.
    Resumed {
        source: SourceId,
        item: String,
        from: u64,
    },
    /// A partial download started over.
    Restarted {
        source: SourceId,
        item: String,
        reason: RestartReason,
    },
    /// The item is complete and on disk.
    Done {
        source: SourceId,
        item: String,
        bytes: u64,
    },
    /// A source was normalized into `published` entries, leaving `left_out` rows out.
    Normalized {
        source: SourceId,
        published: u64,
        left_out: u64,
    },
    /// A source could not be normalized.
    NormalizeFailed {
        source: SourceId,
        error: String,
    },
}

/// Where a run sends its events.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}

/// A sink that keeps every event, for tests and summaries.
#[derive(Debug, Default)]
pub struct RecordingSink(Mutex<Vec<Event>>);

impl RecordingSink {
    /// The events so far, in order.
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: Event) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}
