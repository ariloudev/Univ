//! The id ledger: the ids Univ Ingest has issued, release by release, and what became of every id
//! that left the releases. Univ Server reads it to direct a client from an old id to the objects
//! that replaced it.
//!
//! Ids are issued in ascending order from 1, so the ledger proves that no id is given twice by its
//! high-water mark, the last id issued: every id up to it has been given once, and a new id is
//! always above it. Each release's range of new ids is listed too, so the ledger names the release
//! that issued any id.
//!
//! "Older" between two ids means lower. With ids issued in ascending order that is the id issued
//! in the earlier release, or, within one release, the one issued first.

mod check;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::ObjectId;

/// The version of the ledger file this model reads and writes.
pub const LEDGER_VERSION: u32 = 1;

/// Every id issued and what became of each id replaced. Checked when read and on each release
/// recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLedger")]
pub struct Ledger {
    version: u32,
    /// The high-water mark: the last id issued, or 0 before the first.
    last_issued: u64,
    releases: Vec<Release>,
    replaced: Vec<Replacement>,
}

/// A ledger as read, before its checks.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLedger {
    version: u32,
    last_issued: u64,
    #[serde(default)]
    releases: Vec<Release>,
    #[serde(default)]
    replaced: Vec<Replacement>,
}

/// A release, in the order they were made, with the range of ids it issued, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued: Option<IdRange>,
}

/// The ids from `first` to `last`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdRange {
    pub first: ObjectId,
    pub last: ObjectId,
}

/// What happened to an id that left the releases: in which release, how, and the ids of that
/// release that replaced it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub id: ObjectId,
    pub release: String,
    pub change: Change,
    /// The ids that replaced it, ascending: one when merged, two or more when split, none when
    /// retired.
    pub replaced_by: Vec<ObjectId>,
}

/// How an id left the releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// Its object merged into another object of the previous release, which kept the older id.
    MergedInto,
    /// Its object split, and each part got a new id.
    SplitInto,
    /// Every catalogue entry of its object left the release.
    Retired,
}

/// A ledger that cannot stand.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("ledger version {0} is not the version read here, {LEDGER_VERSION}")]
    Version(u32),
    #[error("release {0:?} is listed twice")]
    RepeatedRelease(String),
    #[error("a release has an empty name")]
    EmptyRelease,
    #[error("the ids issued by release {0:?} do not follow on from the last id issued before it")]
    Range(String),
    #[error("the last id issued is {0}, but the releases issued up to {1}")]
    HighWater(u64, u64),
    #[error("every object id up to 2^53 - 1 has been issued")]
    Exhausted,
    #[error("id {0} was never issued")]
    NotIssued(u64),
    #[error("id {0} is replaced twice")]
    ReplacedTwice(ObjectId),
    #[error("id {0} is replaced in release {1:?}, which is not listed")]
    UnknownRelease(ObjectId, String),
    #[error("id {0} is replaced no later than the release that issued it")]
    ReplacedBeforeIssued(ObjectId),
    #[error("id {0} is recorded as {1:?} with {2} replacing ids")]
    Shape(ObjectId, Change, usize),
    #[error("the ids replacing {0} are not ascending, distinct and other than it")]
    ReplacingIds(ObjectId),
    #[error("id {0} is replaced by {1}, which was not live in that release")]
    NotLive(ObjectId, ObjectId),
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<RawLedger> for Ledger {
    type Error = LedgerError;
    fn try_from(raw: RawLedger) -> Result<Self, LedgerError> {
        let ledger = Self {
            version: raw.version,
            last_issued: raw.last_issued,
            releases: raw.releases,
            replaced: raw.replaced,
        };
        ledger.check()?;
        Ok(ledger)
    }
}

impl Ledger {
    /// The ledger before the first release: nothing issued.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: LEDGER_VERSION,
            last_issued: 0,
            releases: Vec::new(),
            replaced: Vec::new(),
        }
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The high-water mark: the last id issued, if any has been.
    #[must_use]
    pub fn last_issued(&self) -> Option<ObjectId> {
        ObjectId::new(self.last_issued).ok()
    }

    /// The releases, in the order they were made.
    #[must_use]
    pub fn releases(&self) -> &[Release] {
        &self.releases
    }

    /// Every id replaced, release by release.
    #[must_use]
    pub fn replaced(&self) -> &[Replacement] {
        &self.replaced
    }

    /// Whether `id` has been issued: every id up to the high-water mark has, and no other.
    #[must_use]
    pub fn is_issued(&self, id: ObjectId) -> bool {
        id.get() <= self.last_issued
    }

    /// The id that the next object to need one gets.
    ///
    /// # Errors
    ///
    /// Fails when every id up to 2^53 − 1 has been issued.
    pub fn next_id(&self) -> Result<ObjectId, LedgerError> {
        ObjectId::new(self.last_issued + 1).map_err(|_| LedgerError::Exhausted)
    }

    /// The name of the release that issued `id`.
    #[must_use]
    pub fn issued_in(&self, id: ObjectId) -> Option<&str> {
        self.releases
            .iter()
            .find(|r| r.issued.is_some_and(|range| range.contains(id)))
            .map(|r| r.name.as_str())
    }

    /// What happened to `id`, if it left the releases.
    #[must_use]
    pub fn replacement(&self, id: ObjectId) -> Option<&Replacement> {
        self.replaced.iter().find(|r| r.id == id)
    }

    /// The replacements recorded by `release`: the ids it merged, split or retired.
    pub fn replaced_in<'a>(&'a self, release: &'a str) -> impl Iterator<Item = &'a Replacement> {
        self.replaced.iter().filter(move |r| r.release == release)
    }

    /// The ids that stand for `id` now, following its replacements release after release: `id`
    /// itself while it was never replaced, and none when every line from it was retired.
    #[must_use]
    pub fn current(&self, id: ObjectId) -> BTreeSet<ObjectId> {
        let mut current = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![id];
        while let Some(next) = pending.pop() {
            if !seen.insert(next) {
                continue;
            }
            match self.replacement(next) {
                Some(r) => pending.extend(&r.replaced_by),
                None => {
                    current.insert(next);
                }
            }
        }
        current
    }

    /// Records release `name`, which issued the ids of `issued` and replaced the ids of
    /// `replaced`. The ledger is unchanged when the release is refused.
    ///
    /// # Errors
    ///
    /// Fails when the release is already listed, `issued` does not follow on from the high-water
    /// mark, or a replacement does not stand.
    pub fn record_release(
        &mut self,
        name: &str,
        issued: Option<IdRange>,
        mut replaced: Vec<Replacement>,
    ) -> Result<(), LedgerError> {
        let mut next = self.clone();
        next.releases.push(Release {
            name: name.to_owned(),
            issued,
        });
        if let Some(range) = issued {
            next.last_issued = range.last.get();
        }
        replaced.sort_by_key(|r| r.id);
        next.replaced.extend(replaced);
        next.check()?;
        *self = next;
        Ok(())
    }
}

impl IdRange {
    #[must_use]
    pub fn contains(self, id: ObjectId) -> bool {
        (self.first..=self.last).contains(&id)
    }
}
