//! The plain display, for a log or a pipe: timestamped lines and no control codes.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use univ_model::ids::SourceId;

use super::{RunState, Summary, size};
use crate::events::{Event, EventSink};
use crate::fetch::Clock;

/// How often a long download reports its progress.
pub const PROGRESS_EVERY: Duration = Duration::from_secs(60);

struct Inner<W> {
    state: RunState,
    out: W,
    /// When each download in progress last reported, or began.
    reported: BTreeMap<(SourceId, String), SystemTime>,
}

/// The plain display of a run, writing to `W`.
pub struct PlainSink<C: Clock, W: Write + Send> {
    inner: Mutex<Inner<W>>,
    clock: C,
}

impl<C: Clock, W: Write + Send> PlainSink<C, W> {
    pub fn new(clock: C, out: W) -> Self {
        let inner = Inner {
            state: RunState::default(),
            out,
            reported: BTreeMap::new(),
        };
        Self {
            inner: Mutex::new(inner),
            clock,
        }
    }

    /// Prints the summary.
    pub fn finish(&self, summary: &Summary) {
        let stamp = self.stamp();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        for line in summary.lines() {
            let _ = writeln!(inner.out, "{stamp} {}", line.trim_start());
        }
        let _ = inner.out.flush();
    }

    /// The bytes fetched so far.
    pub fn fetched(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .state
            .fetched
    }

    /// Gives back the writer, as tests read what was printed.
    pub fn into_inner(self) -> W {
        self.inner
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .out
    }

    fn stamp(&self) -> String {
        humantime::format_rfc3339_seconds(self.clock.now()).to_string()
    }
}

impl<C: Clock, W: Write + Send> EventSink for PlainSink<C, W> {
    fn emit(&self, event: Event) {
        let now = self.clock.now();
        let stamp = self.stamp();
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let inner = &mut *guard;
        match &event {
            Event::Get { source, item, .. } => {
                inner.reported.insert((source.clone(), item.clone()), now);
            }
            Event::Received {
                source,
                item,
                bytes,
                total,
            } => {
                let key = (source.clone(), item.clone());
                let last = inner.reported.get(&key).copied().unwrap_or(now);
                if now.duration_since(last).unwrap_or_default() >= PROGRESS_EVERY {
                    let of = total.map_or_else(String::new, |t| format!(" of {}", size(t)));
                    let _ = writeln!(
                        inner.out,
                        "{stamp} Progress {source} {item} {}{of}",
                        size(*bytes)
                    );
                    inner.reported.insert(key, now);
                }
            }
            Event::Done { source, item, .. } => {
                inner.reported.remove(&(source.clone(), item.clone()));
            }
            _ => {}
        }
        if let Some(line) = inner.state.apply(&event) {
            let _ = writeln!(inner.out, "{stamp} {line}");
        }
    }
}
