//! The lines of a stage's progress: its start, steps and end, the outcome of each source it
//! normalized, and a progress line for a stage that has printed nothing for a while.

use std::time::{Duration, SystemTime};

use super::{RunState, StageCounts, clock, size, stage_name};
use crate::events::{Count, Event};

impl RunState {
    /// Takes in an event of a stage's progress, which happened at `now`, and gives the line it
    /// prints.
    pub(super) fn stage_event(&mut self, event: &Event, now: SystemTime) -> Option<String> {
        match event {
            Event::StageStarted { stage, items } => {
                self.stage = Some((*stage, 0, *items));
                self.stage_started = Some(now);
                Some(match items {
                    Some(n) => format!("Stage {}: {n} items", stage_name(*stage)),
                    None => format!("Stage {}", stage_name(*stage)),
                })
            }
            Event::StageEnded { stage, counts } => {
                self.stage_started = None;
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
            Event::Uploaded { stage, step, bytes } => {
                self.advance();
                self.uploaded += bytes;
                Some(format!("Step {}: {step}", stage_name(*stage)))
            }
            Event::Noted { stage, line } => Some(format!("Step {}: {line}", stage_name(*stage))),
            Event::Stopping { what } => Some(format!("Stopping: {what}")),
            Event::Normalized { .. } | Event::NormalizeFailed { .. } => {
                self.advance();
                outcome_line(event)
            }
            _ => outcome_line(event),
        }
    }

    /// The progress line of a stage in progress that has printed no line for `every` by
    /// `now`, marked as printed: what a log shows of a long stage once a minute (AC-119).
    pub fn stage_progress_due(&mut self, now: SystemTime, every: Duration) -> Option<String> {
        let ((stage, done, items), started) = (self.stage?, self.stage_started?);
        let quiet_since = self.last_line.unwrap_or(started).max(started);
        if now.duration_since(quiet_since).unwrap_or_default() < every {
            return None;
        }
        self.last_line = Some(now);
        let of = items.map_or_else(|| done.to_string(), |n| format!("{done} of {n}"));
        let moved = if self.uploaded > 0 {
            format!(", {} uploaded", size(self.uploaded))
        } else {
            String::new()
        };
        let took = clock(now.duration_since(started).unwrap_or_default().as_secs());
        Some(format!(
            "Progress {}: {of} items done{moved}, {took} in the stage",
            stage_name(stage)
        ))
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
            point,
        } => {
            let shown = if *point {
                "shown at its published position"
            } else {
                "it publishes no position, so it is not placed"
            };
            Some(format!(
                "Members {structure}: {} of {members} cannot be placed ({no_object} name no \
                 matched object, {unplaced} an unplaced one); {shown}",
                no_object + unplaced
            ))
        }
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
