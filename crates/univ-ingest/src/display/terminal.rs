//! The terminal display: event lines scroll above a live status line, as apt shows a download.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};

use super::{Outcome, RunDisplay, RunState, SourceSummary, Summary};
use crate::events::{Event, EventSink};
use crate::fetch::Clock;

/// The time between two draws of the status line: ten a second.
pub const REDRAW_EVERY: Duration = Duration::from_millis(100);

/// Where the terminal display draws: a real terminal, or a recorder in tests.
pub trait StatusTarget: Send + Sync {
    /// Prints `lines` above the status line and redraws it as `status`, in one draw.
    fn draw(&self, lines: &[String], status: &str);
    /// The terminal's width in characters.
    fn width(&self) -> usize;
    /// Removes the status line and prints `lines` in its place, once the run ends.
    fn finish(&self, lines: &[String]);
}

/// A status line drawn by indicatif on standard error.
///
/// The status is a template key read from `status` rather than the bar's message, because
/// `ProgressBar::set_message` draws by itself (at most at the target's rate) and `println`
/// always draws: setting the message and then printing would draw twice.
pub struct IndicatifTarget {
    bar: ProgressBar,
    status: Arc<Mutex<String>>,
}

impl IndicatifTarget {
    #[must_use]
    pub fn stderr() -> Self {
        let status = Arc::new(Mutex::new(String::new()));
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr_with_hz(10));
        let shown = status.clone();
        let key = move |_: &ProgressState, out: &mut dyn std::fmt::Write| {
            let _ = out.write_str(&shown.lock().unwrap_or_else(PoisonError::into_inner));
        };
        if let Ok(style) = ProgressStyle::with_template("{status}") {
            bar.set_style(style.with_key("status", key));
        }
        Self { bar, status }
    }
}

impl StatusTarget for IndicatifTarget {
    fn draw(&self, lines: &[String], status: &str) {
        status.clone_into(&mut self.status.lock().unwrap_or_else(PoisonError::into_inner));
        if lines.is_empty() {
            // One draw, which the target's rate limit may drop.
            self.bar.tick();
        } else {
            // One forced draw: the lines, then the status line below them.
            self.bar.println(lines.join("\n"));
        }
    }

    fn width(&self) -> usize {
        usize::from(console::Term::stderr().size().1)
    }

    fn finish(&self, lines: &[String]) {
        self.bar.finish_and_clear();
        let mut text = String::new();
        for line in lines {
            let _ = writeln!(text, "{line}");
        }
        eprint!("{text}");
    }
}

struct Inner {
    state: RunState,
    /// Lines of events since the last draw.
    queued: Vec<String>,
}

/// The terminal display of a run. Events only update its state; `draw` shows them.
pub struct TerminalSink<C: Clock, T: StatusTarget> {
    inner: Mutex<Inner>,
    clock: C,
    started: SystemTime,
    target: T,
}

impl<C: Clock, T: StatusTarget> TerminalSink<C, T> {
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

    fn elapsed(&self) -> Duration {
        self.clock
            .now()
            .duration_since(self.started)
            .unwrap_or_default()
    }
}

impl<C: Clock, T: StatusTarget> EventSink for TerminalSink<C, T> {
    fn emit(&self, event: Event) {
        let now = self.clock.now();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(line) = inner.state.apply(&event, now) {
            inner.queued.push(line);
        }
    }
}

impl<C: Clock, T: StatusTarget> RunDisplay for TerminalSink<C, T> {
    /// Prints the lines queued since the last draw and redraws the status line, in one draw.
    fn draw(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let lines = std::mem::take(&mut inner.queued);
        let status = inner.state.status(self.elapsed(), self.target.width());
        self.target.draw(&lines, &status);
    }

    fn finish(&self, sources: Vec<SourceSummary>, outcome: Outcome) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let summary = Summary {
            bytes: inner.state.fetched,
            elapsed: self.elapsed(),
            sources,
            outcome,
        };
        let mut lines = std::mem::take(&mut inner.queued);
        lines.extend(summary.lines());
        self.target.finish(&lines);
    }
}
