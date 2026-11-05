//! The plain target, for a log or a pipe: timestamped lines and no control codes.

use std::io::Write;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use super::Target;

/// How often a long download reports its progress.
pub const PROGRESS_EVERY: Duration = Duration::from_secs(60);

/// Plain lines written to `W`, each stamped with the time it was printed.
pub struct PlainTarget<W>(Mutex<W>);

impl<W: Write + Send> PlainTarget<W> {
    pub fn new(out: W) -> Self {
        Self(Mutex::new(out))
    }

    /// Gives back the writer, as tests read what was printed.
    pub fn into_inner(self) -> W {
        self.0.into_inner().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, now: SystemTime, lines: &[String]) {
        let stamp = humantime::format_rfc3339_seconds(now);
        let mut out = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        for line in lines {
            let _ = writeln!(out, "{stamp} {}", line.trim_start());
        }
    }
}

impl<W: Write + Send> Target for PlainTarget<W> {
    fn live(&self) -> bool {
        false
    }

    fn print(&self, now: SystemTime, lines: &[String]) {
        self.write(now, lines);
    }

    fn finish(&self, now: SystemTime, lines: &[String]) {
        self.write(now, lines);
        let _ = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flush();
    }
}
