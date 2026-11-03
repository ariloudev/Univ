//! The one display of a run, over a target: a terminal that keeps a live status line, or plain
//! lines for a log.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use super::plain::{PROGRESS_EVERY, PlainTarget};
use super::{Outcome, RunDisplay, RunState, SourceSummary, StatusTarget, Summary};
use crate::events::{Event, EventSink};
use crate::fetch::Clock;

/// Where a run's display prints.
pub trait Target: Send + Sync {
    /// Whether the target keeps a live status line: event lines then wait for its next draw.
    /// A target that is not live prints them as they come, with a progress line for each long
    /// download.
    fn live(&self) -> bool;
    /// Prints `lines`, which happened at `now`, as they come.
    fn print(&self, _now: SystemTime, _lines: &[String]) {}
    /// Prints `lines` above the status line and redraws it as `status`, in one draw.
    fn draw(&self, _lines: &[String], _status: &str) {}
    /// The width the status line must fit, in characters.
    fn width(&self) -> usize {
        usize::MAX
    }
    /// Prints the lines that end the run, at `now`.
    fn finish(&self, now: SystemTime, lines: &[String]);
}

impl<T: StatusTarget> Target for T {
    fn live(&self) -> bool {
        true
    }

    fn draw(&self, lines: &[String], status: &str) {
        StatusTarget::draw(self, lines, status);
    }

    fn width(&self) -> usize {
        StatusTarget::width(self)
    }

    fn finish(&self, _: SystemTime, lines: &[String]) {
        StatusTarget::finish(self, lines);
    }
}

struct Inner {
    state: RunState,
    /// Lines of events since the last draw, for a live target.
    queued: Vec<String>,
}

/// The display of a run over `T`: events update its state, and print as the target prints.
pub struct RunSink<C: Clock, T: Target> {
    inner: Mutex<Inner>,
    clock: C,
    started: SystemTime,
    target: T,
}

impl<C: Clock, T: Target> RunSink<C, T> {
    pub fn new(clock: C, target: T) -> Self {
        let started = clock.now();
        let inner = Inner {
            state: RunState::default(),
            queued: Vec::new(),
        };
        Self {
            inner: Mutex::new(inner),
            clock,
            started,
            target,
        }
    }

    fn elapsed(&self, now: SystemTime) -> Duration {
        now.duration_since(self.started).unwrap_or_default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<C: Clock, W: std::io::Write + Send> RunSink<C, PlainTarget<W>> {
    /// Gives back the plain target's writer, as tests read what was printed.
    pub fn into_inner(self) -> W {
        self.target.into_inner()
    }
}

impl<C: Clock, T: Target> EventSink for RunSink<C, T> {
    fn emit(&self, event: Event) {
        let now = self.clock.now();
        let mut inner = self.lock();
        let line = inner.state.apply(&event, now);
        if self.target.live() {
            inner.queued.extend(line);
        } else {
            let mut lines: Vec<String> = line.into_iter().collect();
            lines.extend(inner.state.progress_due(now, PROGRESS_EVERY));
            self.target.print(now, &lines);
        }
    }
}

impl<C: Clock, T: Target> RunDisplay for RunSink<C, T> {
    /// Prints the lines queued since the last draw and redraws the status line, in one draw.
    fn draw(&self) {
        if !self.target.live() {
            return;
        }
        let now = self.clock.now();
        let mut inner = self.lock();
        let lines = std::mem::take(&mut inner.queued);
        let status = inner.state.status(self.elapsed(now), self.target.width());
        self.target.draw(&lines, &status);
    }

    fn finish(&self, sources: Vec<SourceSummary>, outcome: Outcome) {
        let now = self.clock.now();
        let mut inner = self.lock();
        let summary = Summary {
            bytes: inner.state.fetched,
            elapsed: self.elapsed(now),
            sources,
            stages: inner.state.stages.clone(),
            outcome,
        };
        let mut lines = std::mem::take(&mut inner.queued);
        lines.extend(summary.lines());
        self.target.finish(now, &lines);
    }
}
