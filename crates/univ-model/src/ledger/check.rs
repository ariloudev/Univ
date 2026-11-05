//! The checks a ledger passes when read and on each release recorded.

use std::collections::{BTreeMap, HashMap};

use super::{Change, LEDGER_VERSION, Ledger, LedgerError, Replacement};
use crate::ids::ObjectId;

/// The releases of a ledger, checked: each one's position by name, and the last id and position
/// of each release that issued ids.
struct Releases<'a> {
    index: HashMap<&'a str, usize>,
    issued: Vec<(u64, usize)>,
}

impl Ledger {
    /// Checks the version, that the releases issued ids 1 to the high-water mark in turn, and that
    /// each replacement names an issued id, a listed release after the one that issued it, and
    /// ids that were live in that release.
    pub(super) fn check(&self) -> Result<(), LedgerError> {
        if self.version != LEDGER_VERSION {
            return Err(LedgerError::Version(self.version));
        }
        let releases = self.check_releases()?;
        let mut replaced_in: BTreeMap<ObjectId, usize> = BTreeMap::new();
        for r in &self.replaced {
            let release = *releases
                .index
                .get(r.release.as_str())
                .ok_or_else(|| LedgerError::UnknownRelease(r.id, r.release.clone()))?;
            if replaced_in.insert(r.id, release).is_some() {
                return Err(LedgerError::ReplacedTwice(r.id));
            }
        }
        for r in &self.replaced {
            let release = replaced_in[&r.id];
            if releases.issued_by(r.id)? >= release {
                return Err(LedgerError::ReplacedBeforeIssued(r.id));
            }
            check_shape(r)?;
            for &by in &r.replaced_by {
                let live = releases.issued_by(by)? <= release
                    && replaced_in.get(&by).is_none_or(|&later| later > release);
                if !live {
                    return Err(LedgerError::NotLive(r.id, by));
                }
            }
        }
        Ok(())
    }

    /// Checks the release names and that their ranges issue ids 1 to the high-water mark in turn.
    fn check_releases(&self) -> Result<Releases<'_>, LedgerError> {
        let mut index = HashMap::new();
        let mut issued = Vec::new();
        let mut issued_to = 0;
        for (i, release) in self.releases.iter().enumerate() {
            if release.name.trim().is_empty() {
                return Err(LedgerError::EmptyRelease);
            }
            if index.insert(release.name.as_str(), i).is_some() {
                return Err(LedgerError::RepeatedRelease(release.name.clone()));
            }
            if let Some(range) = release.issued {
                if range.first.get() != issued_to + 1 || range.last < range.first {
                    return Err(LedgerError::Range(release.name.clone()));
                }
                issued_to = range.last.get();
                issued.push((issued_to, i));
            }
        }
        if issued_to != self.last_issued {
            return Err(LedgerError::HighWater(self.last_issued, issued_to));
        }
        Ok(Releases { index, issued })
    }
}

impl Releases<'_> {
    /// The position of the release that issued `id`.
    fn issued_by(&self, id: ObjectId) -> Result<usize, LedgerError> {
        let at = self.issued.partition_point(|&(last, _)| last < id.get());
        self.issued
            .get(at)
            .map(|&(_, release)| release)
            .ok_or(LedgerError::NotIssued(id.get()))
    }
}

/// Checks the number of replacing ids for the change, and that they are ascending, distinct and
/// other than the id replaced.
fn check_shape(r: &Replacement) -> Result<(), LedgerError> {
    let count = r.replaced_by.len();
    let fits = match r.change {
        Change::MergedInto => count == 1,
        Change::SplitInto => count >= 2,
        Change::Retired => count == 0,
    };
    if !fits {
        return Err(LedgerError::Shape(r.id, r.change, count));
    }
    let ordered = r.replaced_by.windows(2).all(|w| w[0] < w[1]);
    if !ordered || r.replaced_by.contains(&r.id) {
        return Err(LedgerError::ReplacingIds(r.id));
    }
    Ok(())
}
