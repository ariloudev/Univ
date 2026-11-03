//! The terminal display: event lines scroll above a live status line, as apt shows a download.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::{RunState, Summary};
use crate::events::{Event, EventSink};
use crate::fetch::Clock;

/// The shortest time between two redraws of the status line: at most ten a second.
pub const REDRAW_EVERY: Duration = Duration::from_millis(100);

/// Where the terminal display draws: a real terminal, or a recorder in tests.
pub trait StatusTarget: Send + Sync {
    /// Prints a line above the status line.
    fn line(&self, line: &str);
    /// Replaces the status line.
    fn status(&self, status: &str);
    /// The terminal's width in characters.
    fn width(&self) -> usize;
    /// Removes the status line once the run ends.
    fn finish(&self);
}

/// A status line drawn by indicatif on standard error.
pub struct IndicatifTarget(ProgressBar);

impl IndicatifTarget {
    #[must_use]
    pub fn stderr() -> Self {
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        if let Ok(style) = ProgressStyle::with_template("{msg}") {
            bar.set_style(style);
        }
        Self(bar)
    }
}

impl StatusTarget for IndicatifTarget {
    fn line(&self, line: &str) {
        self.0.println(line);
    }

    fn status(&self, status: &str) {
        self.0.set_message(status.to_owned());
    }

    fn width(&self) -> usize {
        usize::from(console::Term::stderr().size().1)
    }

    fn finish(&self) {
        self.0.finish_and_clear();
    }
}

/// The terminal display of a run.
pub struct TerminalSink<C: Clock, T: StatusTarget> {
    state: Mutex<(RunState, Option<SystemTime>)>,
    clock: C,
    started: SystemTime,
    target: T,
}

impl<C: Clock, T: StatusTarget> TerminalSink<C, T> {
    pub fn new(clock: C, target: T) -> Self {
        let started = clock.now();
        Self {
            state: Mutex::default(),
            clock,
            started,
            target,
        }
    }

    /// Clears the status line and prints the summary.
    pub fn finish(&self, summary: &Summary) {
        self.target.finish();
        for line in summary.lines() {
            self.target.line(&line);
        }
    }

    /// The bytes fetched so far.
    pub fn fetched(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .0
            .fetched
    }
}

impl<C: Clock, T: StatusTarget> EventSink for TerminalSink<C, T> {
    fn emit(&self, event: Event) {
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let (state, last) = &mut *guard;
        if let Some(line) = state.apply(&event) {
            self.target.line(&line);
        }
        let now = self.clock.now();
        let due = last.is_none_or(|l| now.duration_since(l).unwrap_or_default() >= REDRAW_EVERY);
        if due {
            let elapsed = now.duration_since(self.started).unwrap_or_default();
            self.target
                .status(&state.status(elapsed, self.target.width()));
            *last = Some(now);
        }
    }
}
