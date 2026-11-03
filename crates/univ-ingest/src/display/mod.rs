//! What a run shows: the state built from its events, the line each event prints and the status
//! line, all as plain text so that the terminal and plain sinks share them.

mod plain;
mod summary;
mod terminal;

use std::collections::BTreeMap;
use std::time::Duration;

use univ_model::ids::SourceId;

use crate::events::{Event, RestartReason, Stage};

pub use plain::PlainSink;
pub use summary::{Outcome, SourceSummary, Summary};
pub use terminal::{IndicatifTarget, StatusTarget, TerminalSink};

/// A download in progress, as the status line shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Download {
    number: u64,
    bytes: u64,
    total: Option<u64>,
}

/// The state of a run, built from its events.
#[derive(Debug, Default)]
pub struct RunState {
    numbers: BTreeMap<(SourceId, String), u64>,
    next: u64,
    downloads: BTreeMap<(SourceId, String), Download>,
    stage: Option<(Stage, u64, Option<u64>)>,
    /// Bytes of finished items, and their sizes counted as expected.
    finished: u64,
    /// Bytes the run has fetched in all, finished or not.
    pub fetched: u64,
}

impl RunState {
    /// Takes in `event` and gives the line it prints, if it prints one.
    pub fn apply(&mut self, event: &Event) -> Option<String> {
        match event {
            Event::StageStarted { stage, items } => {
                self.stage = Some((*stage, 0, *items));
                Some(match items {
                    Some(n) => format!("Stage {}: {n} items", stage_name(*stage)),
                    None => format!("Stage {}", stage_name(*stage)),
                })
            }
            Event::StageEnded { stage } => Some(format!("Stage {} done", stage_name(*stage))),
            Event::Get { source, item, .. } => {
                let number = self.number(source, item);
                self.downloads.insert(
                    key(source, item),
                    Download {
                        number,
                        bytes: 0,
                        total: None,
                    },
                );
                None
            }
            Event::Received {
                source,
                item,
                bytes,
                total,
            } => {
                if let Some(d) = self.downloads.get_mut(&key(source, item)) {
                    self.fetched += bytes.saturating_sub(d.bytes);
                    d.bytes = *bytes;
                    d.total = *total;
                }
                None
            }
            Event::Done {
                source,
                item,
                bytes,
            } => {
                let number = self.number(source, item);
                let streamed = self
                    .downloads
                    .remove(&key(source, item))
                    .map_or(0, |d| d.bytes);
                self.fetched += bytes.saturating_sub(streamed);
                self.finished += bytes;
                self.advance();
                Some(format!("Get:{number} {source} {item} [{}]", size(*bytes)))
            }
            Event::Hit { source, item } => {
                let number = self.number(source, item);
                self.advance();
                Some(format!("Hit:{number} {source} {item}"))
            }
            Event::Err {
                source,
                item,
                mirror,
                error,
                retry_in,
                ..
            } => {
                let number = self.number(source, item);
                if retry_in.is_none() {
                    self.downloads.remove(&key(source, item));
                }
                let next = retry_in.map_or_else(
                    || "trying the next mirror".to_owned(),
                    |wait| format!("retrying in {} s", wait.as_secs()),
                );
                Some(format!(
                    "Err:{number} {source} {item} {mirror}: {error}; {next}"
                ))
            }
            Event::MirrorSkipped { source, mirror } => Some(format!(
                "Skip {mirror} for {source} and the rest of the run: it failed all its retries"
            )),
            Event::Resumed { source, item, from } => {
                let number = self.number(source, item);
                if let Some(d) = self.downloads.get_mut(&key(source, item)) {
                    d.bytes = *from;
                }
                Some(format!("Resume:{number} {source} {item} from byte {from}"))
            }
            Event::Restarted {
                source,
                item,
                reason,
            } => {
                let number = self.number(source, item);
                let why = match reason {
                    RestartReason::RangeIgnored => "the provider ignored the range asked for",
                    RestartReason::Changed => "the file changed since the partial download",
                    RestartReason::RangeUnsatisfiable => "the provider could not serve the range",
                };
                Some(format!(
                    "Restart:{number} {source} {item} from the first byte: {why}"
                ))
            }
        }
    }

    /// The status line after `elapsed`, cut to `width` characters.
    #[must_use]
    pub fn status(&self, elapsed: Duration, width: usize) -> String {
        let in_flight: u64 = self.downloads.values().filter_map(|d| d.total).sum();
        let expected = self.finished + in_flight;
        let mut parts = Vec::new();
        if let Some((stage, done, items)) = self.stage {
            let percent = items
                .filter(|n| *n > 0)
                .map_or_else(String::new, |n| format!("{}% ", done * 100 / n));
            let of = items.map_or_else(|| done.to_string(), |n| format!("{done}/{n}"));
            parts.push(format!("{percent}[{} {of}]", stage_name(stage)));
        }
        let of = if expected > 0 {
            format!(" of {}", size(expected))
        } else {
            String::new()
        };
        let left = if expected > self.fetched && self.fetched > 0 {
            // Time left at the mean rate so far: remaining × elapsed / fetched.
            let millis = u128::from(expected - self.fetched) * elapsed.as_millis()
                / u128::from(self.fetched);
            format!(
                " {} left",
                clock(u64::try_from(millis / 1_000).unwrap_or(u64::MAX))
            )
        } else {
            String::new()
        };
        parts.push(format!(
            "{}{of} {}/s{left}",
            size(self.fetched),
            size(rate(self.fetched, elapsed))
        ));
        for ((source, item), d) in &self.downloads {
            let percent = d
                .total
                .filter(|t| *t > 0)
                .map_or_else(String::new, |t| format!(" {}%", d.bytes * 100 / t));
            parts.push(format!(
                "{}:{source} {item} {}{percent}",
                d.number,
                size(d.bytes)
            ));
        }
        cut(&parts.join(" | "), width)
    }

    fn number(&mut self, source: &SourceId, item: &str) -> u64 {
        let next = &mut self.next;
        *self.numbers.entry(key(source, item)).or_insert_with(|| {
            *next += 1;
            *next
        })
    }

    fn advance(&mut self) {
        if let Some((_, done, _)) = &mut self.stage {
            *done += 1;
        }
    }
}

fn key(source: &SourceId, item: &str) -> (SourceId, String) {
    (source.clone(), item.to_owned())
}

/// The stage's name as the owner reads it.
#[must_use]
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Fetch => "fetch",
        Stage::Normalize => "normalize",
        Stage::Match => "cross-match",
        Stage::Tiles => "tiles",
        Stage::Release => "build",
        Stage::Publish => "publish",
    }
}

/// The mean rate in bytes a second of `bytes` over `elapsed`.
#[must_use]
pub fn rate(bytes: u64, elapsed: Duration) -> u64 {
    let millis = elapsed.as_millis();
    if millis == 0 {
        return 0;
    }
    u64::try_from(u128::from(bytes) * 1_000 / millis).unwrap_or(u64::MAX)
}

/// A size in decimal units, as apt prints them: B, kB, MB, GB, TB, to a tenth, cut not rounded.
#[must_use]
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    if bytes < 1_000 {
        return format!("{bytes} B");
    }
    let mut scale: u64 = 1_000;
    let mut unit = 0;
    while bytes / scale >= 1_000 && unit + 1 < UNITS.len() {
        scale *= 1_000;
        unit += 1;
    }
    let tenths = u128::from(bytes) * 10 / u128::from(scale);
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

/// A duration in seconds as `1h02m`, `3m05s` or `42s`.
#[must_use]
pub fn clock(seconds: u64) -> String {
    match (seconds / 3_600, seconds / 60 % 60, seconds % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m{s:02}s"),
        (h, m, _) => format!("{h}h{m:02}m"),
    }
}

/// `text` cut to `width` characters, an ellipsis marking the cut.
#[must_use]
pub fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}
