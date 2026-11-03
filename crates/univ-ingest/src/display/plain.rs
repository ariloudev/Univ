//! The plain display, for a log or a pipe: timestamped lines and no control codes.

use std::io::Write;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use super::{Outcome, RunDisplay, RunState, SourceSummary, Summary};
use crate::events::{Event, EventSink};
use crate::fetch::Clock;

/// How often a long download reports its progress.
pub const PROGRESS_EVERY: Duration = Duration::from_secs(60);

struct Inner<W> {
    state: RunState,
    out: W,
}

impl<W: Write> Inner<W> {
    /// Writes `line` stamped with `now`.
    fn write(&mut self, now: SystemTime, line: &str) {
        let stamp = humantime::format_rfc3339_seconds(now);
        let _ = writeln!(self.out, "{stamp} {line}");
    }
}

/// The plain display of a run, writing to `W`.
pub struct PlainSink<C: Clock, W: Write + Send> {
    inner: Mutex<Inner<W>>,
    clock: C,
    started: SystemTime,
}

impl<C: Clock, W: Write + Send> PlainSink<C, W> {
    pub fn new(clock: C, out: W) -> Self {
        let inner = Inner {
            state: RunState::default(),
            out,
        };
        let started = clock.now();
        Self {
            inner: Mutex::new(inner),
            clock,
            started,
        }
    }

    /// Gives back the writer, as tests read what was printed.
    pub fn into_inner(self) -> W {
        self.inner
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .out
    }
}

impl<C: Clock, W: Write + Send> EventSink for PlainSink<C, W> {
    fn emit(&self, event: Event) {
        let now = self.clock.now();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(line) = inner.state.apply(&event, now) {
            inner.write(now, &line);
        }
        for line in inner.state.progress_due(now, PROGRESS_EVERY) {
            inner.write(now, &line);
        }
    }
}

impl<C: Clock, W: Write + Send> RunDisplay for PlainSink<C, W> {
    fn finish(&self, sources: Vec<SourceSummary>, outcome: Outcome) {
        let now = self.clock.now();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let summary = Summary {
            bytes: inner.state.fetched,
            elapsed: now.duration_since(self.started).unwrap_or_default(),
            sources,
            outcome,
        };
        for line in summary.lines() {
            inner.write(now, line.trim_start());
        }
        let _ = inner.out.flush();
    }
}
