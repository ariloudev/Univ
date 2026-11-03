//! What a run shows: the state built from its events, the line each event prints and the status
//! line, all as plain text so that the terminal and plain targets share them.

mod plain;
mod sink;
mod summary;
mod terminal;

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime};

use univ_model::ids::SourceId;

use crate::events::{Count, Event, EventSink, RestartReason, Stage};

pub use plain::{PROGRESS_EVERY, PlainTarget};
pub use sink::{RunSink, Target};
pub use summary::{Outcome, SourceState, SourceSummary, StageCounts, Summary};
pub use terminal::{IndicatifTarget, REDRAW_EVERY, StatusTarget};

/// A display of a run: the events it is sent, redrawn while the run lasts, and the summary it
/// prints when the run ends.
pub trait RunDisplay: EventSink {
    /// Brings the display up to date. A display that prints each line as it comes has nothing
    /// to draw.
    fn draw(&self) {}
    /// Prints the summary of the run, which ended with `outcome`: its bytes and time come from
    /// the display's own count and clock.
    fn finish(&self, sources: Vec<SourceSummary>, outcome: Outcome);
}

/// A download in progress, as the status line shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Download {
    source: SourceId,
    item: String,
    /// Bytes on disk when the download started, from a partial download it continues.
    from: u64,
    /// Bytes the item's file holds, counting the ones it started from.
    bytes: u64,
    /// The whole item's size, when the provider gives it.
    total: Option<u64>,
    /// When the download last reported its progress, or began.
    last_reported: SystemTime,
}

/// The state of a run, built from its events.
#[derive(Debug, Default)]
pub struct RunState {
    numbers: HashMap<SourceId, HashMap<String, u64>>,
    next: u64,
    /// Downloads in progress by item number, so in the order they were first seen.
    downloads: BTreeMap<u64, Download>,
    stage: Option<(Stage, u64, Option<u64>)>,
    /// Bytes the finished items took in this run, which is what they counted as expected.
    finished: u64,
    /// Bytes the run has fetched in all, finished or not, not counting what was already on
    /// disk.
    fetched: u64,
    /// What each stage that ended counted, in the order they ended.
    stages: Vec<StageCounts>,
}

impl RunState {
    /// Takes in `event`, which happened at `now`, and gives the line it prints, if it prints
    /// one.
    pub fn apply(&mut self, event: &Event, now: SystemTime) -> Option<String> {
        match event {
            Event::Get { source, item, .. } => {
                self.started(source, item, now);
                None
            }
            Event::Received {
                source,
                item,
                bytes,
                total,
            } => {
                let number = self.number(source, item);
                if let Some(d) = self.downloads.get_mut(&number) {
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
                let (streamed, from) = self
                    .downloads
                    .remove(&number)
                    .map_or((0, 0), |d| (d.bytes, d.from));
                self.fetched += bytes.saturating_sub(streamed);
                self.finished += bytes.saturating_sub(from);
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
                    self.downloads.remove(&number);
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
                if let Some(d) = self.downloads.get_mut(&number) {
                    d.from = *from;
                    d.bytes = *from;
                    d.last_reported = now;
                }
                Some(format!("Resume:{number} {source} {item} from byte {from}"))
            }
            Event::StageStarted { .. }
            | Event::StageEnded { .. }
            | Event::Stepped { .. }
            | Event::Normalized { .. }
            | Event::NormalizeFailed { .. }
            | Event::MembersUnplaced { .. } => self.stage_event(event),
            Event::Restarted {
                source,
                item,
                reason,
            } => {
                let number = self.number(source, item);
                if let Some(d) = self.downloads.get_mut(&number) {
                    d.from = 0;
                    d.bytes = 0;
                }
                Some(format!(
                    "Restart:{number} {source} {item} from the first byte: {}",
                    why(*reason)
                ))
            }
        }
    }

    /// Takes in an event of a stage's progress, and gives the line it prints.
    fn stage_event(&mut self, event: &Event) -> Option<String> {
        match event {
            Event::StageStarted { stage, items } => {
                self.stage = Some((*stage, 0, *items));
                Some(match items {
                    Some(n) => format!("Stage {}: {n} items", stage_name(*stage)),
                    None => format!("Stage {}", stage_name(*stage)),
                })
            }
            Event::StageEnded { stage, counts } => {
                if !counts.is_empty() {
                    self.stages.push(StageCounts {
                        stage: *stage,
                        counts: counts.clone(),
                    });
                }
                Some(format!(
                    "Stage {} done{}",
                    stage_name(*stage),
                    counted(counts)
                ))
            }
            Event::Stepped { stage, step } => {
                self.advance();
                Some(format!("Step {}: {step}", stage_name(*stage)))
            }
            Event::Normalized { .. } | Event::NormalizeFailed { .. } => {
                self.advance();
                outcome_line(event)
            }
            _ => outcome_line(event),
        }
    }

    /// A download of the item begins at `now`, from its first byte.
    fn started(&mut self, source: &SourceId, item: &str, now: SystemTime) {
        let download = Download {
            source: source.clone(),
            item: item.to_owned(),
            from: 0,
            bytes: 0,
            total: None,
            last_reported: now,
        };
        let number = self.number(source, item);
        self.downloads.insert(number, download);
    }

    /// The progress lines of the downloads that have not reported for `every` by `now`, each
    /// marked as reported.
    pub fn progress_due(&mut self, now: SystemTime, every: Duration) -> Vec<String> {
        self.downloads
            .values_mut()
            .filter(|d| now.duration_since(d.last_reported).unwrap_or_default() >= every)
            .map(|d| {
                d.last_reported = now;
                let of = d
                    .total
                    .map_or_else(String::new, |t| format!(" of {}", size(t)));
                format!("Progress {} {} {}{of}", d.source, d.item, size(d.bytes))
            })
            .collect()
    }

    /// The status line after `elapsed`, cut to `width` characters.
    #[must_use]
    pub fn status(&self, elapsed: Duration, width: usize) -> String {
        let in_flight: u64 = self
            .downloads
            .values()
            .filter_map(|d| d.total.map(|t| t.saturating_sub(d.from)))
            .sum();
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
        for (number, d) in &self.downloads {
            let percent = d
                .total
                .filter(|t| *t > 0)
                .map_or_else(String::new, |t| format!(" {}%", d.bytes * 100 / t));
            parts.push(format!(
                "{number}:{} {} {}{percent}",
                d.source,
                d.item,
                size(d.bytes)
            ));
        }
        cut(&parts.join(" | "), width)
    }

    /// The item's number, given the first time the run sees it.
    fn number(&mut self, source: &SourceId, item: &str) -> u64 {
        if let Some(number) = self.numbers.get(source).and_then(|items| items.get(item)) {
            return *number;
        }
        self.next += 1;
        let number = self.next;
        if let Some(items) = self.numbers.get_mut(source) {
            items.insert(item.to_owned(), number);
        } else {
            let items = HashMap::from([(item.to_owned(), number)]);
            self.numbers.insert(source.clone(), items);
        }
        number
    }

    fn advance(&mut self) {
        if let Some((_, done, _)) = &mut self.stage {
            *done += 1;
        }
    }
}

/// `counts` as a stage's end line gives them: `: 98 objects, 3 merges`, or nothing.
#[must_use]
pub fn counted(counts: &[Count]) -> String {
    if counts.is_empty() {
        return String::new();
    }
    let each: Vec<String> = counts
        .iter()
        .map(|c| format!("{} {}", c.n, c.what))
        .collect();
    format!(": {}", each.join(", "))
}

/// The line a normalized source, or a structure whose members cannot all be placed, prints.
fn outcome_line(event: &Event) -> Option<String> {
    match event {
        Event::MembersUnplaced {
            structure,
            members,
            no_object,
            unplaced,
        } => Some(format!(
            "Members {structure}: {} of {members} cannot be placed ({no_object} name no matched              object, {unplaced} an unplaced one); shown at its published position",
            no_object + unplaced
        )),
        Event::Normalized {
            source,
            published,
            left_out,
        } => Some(format!(
            "Norm {source}: {published} entries, {left_out} left out"
        )),
        Event::NormalizeFailed { source, error } => {
            Some(format!("Err {source} cannot be normalized: {error}"))
        }
        _ => None,
    }
}

/// Why a download started over, as the owner reads it.
fn why(reason: RestartReason) -> &'static str {
    match reason {
        RestartReason::RangeIgnored => "the provider ignored the range asked for",
        RestartReason::Changed => "the file changed since the partial download",
        RestartReason::RangeUnsatisfiable => "the provider could not serve the range",
    }
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
