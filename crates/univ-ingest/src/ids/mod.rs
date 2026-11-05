//! Stable object ids (RQ-31, RQ-32): each object of a release gets its id from the catalogue
//! entries it carries over from the previous release, and the id ledger records every id merged,
//! split or retired.
//!
//! The rules, for an object of this release and the previous objects its entries belonged to:
//!
//! - A previous object whose carried-over entries all went to one object of this release goes to
//!   it whole. An object that took one previous object whole keeps its id, whatever new entries
//!   joined it and whatever entries it lost (AC-75, AC-78).
//! - An object that took two or more previous objects whole keeps the older id, the lowest, and
//!   the others are merged into it (AC-76), three or more alike.
//! - A previous object whose carried-over entries went to two or more objects is split into
//!   them, and its id goes on in none of them (AC-77). An object whose previous objects were all
//!   split gets a new id; one that also took a previous object whole keeps that object's id, so
//!   a merge and a split in one release resolve together.
//! - An object of entries new in this release gets a new id (AC-79).
//! - A previous object none of whose entries is in this release is retired.
//! - New ids are issued above the ledger's high-water mark, in the order of each object's
//!   smallest entry key, so no id is given twice (AC-80) and the same groups give the same ids
//!   in whatever order they come.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use thiserror::Error;
use univ_model::ids::{Designation, ObjectId};
use univ_model::ledger::{Change, IdRange, Ledger, LedgerError, Replacement};

pub use crate::state::{read_ledger, write_ledger};

/// A release's map from each catalogue entry, by its source and identifier, to its object's id.
pub type EntryIds = BTreeMap<Designation, ObjectId>;

/// The ids of one release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assigned {
    /// The objects, in the order of their smallest entry key. The cross-match lets them go,
    /// as its release objects hold their ids: they are empty in its output.
    pub objects: Vec<AssignedObject>,
    /// The map from entry to object id, for the next release.
    pub entries: EntryIds,
    /// The ids this release issued, if any.
    pub issued: Option<IdRange>,
    /// The ids this release merged, split or retired, ascending: what the release manifest lists.
    pub replaced: Vec<Replacement>,
}

/// An object of the release with its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignedObject {
    pub id: ObjectId,
    /// Its catalogue entries, sorted.
    pub entries: Vec<Designation>,
    pub origin: Origin,
}

/// Where an object's id came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The id of the one previous object it took whole.
    Kept,
    /// The older id of the previous objects it took whole; the others, listed, merged into it.
    Merged(Vec<ObjectId>),
    /// A new id: its entries are all new, or every previous object they come from was split.
    New,
}

/// Ids that cannot be assigned.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssignError {
    #[error("object {0} of the release has no catalogue entries")]
    EmptyObject(usize),
    #[error("catalogue entry {} {} is in two objects", .0.source, .0.identifier)]
    EntryInTwoObjects(Designation),
    #[error("the previous release holds id {0}, which the ledger records as never issued")]
    NotIssued(ObjectId),
    #[error("the previous release holds id {0}, which the ledger records as replaced")]
    Replaced(ObjectId),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
}

/// Gives each object of release `release` its id, from `previous`, the previous release's map
/// from entry to id, and records the release in `ledger`. `objects` is the release's grouping of
/// catalogue entries into objects, in any order. The ledger is unchanged on failure.
///
/// # Errors
///
/// Fails when an object has no entries, an entry is in two objects, the previous release holds
/// an id the ledger never issued or records as replaced, the release is already in the ledger, or
/// the ids run out.
pub fn assign(
    previous: &EntryIds,
    ledger: &mut Ledger,
    release: &str,
    objects: impl IntoIterator<Item = Vec<Designation>>,
) -> Result<Assigned, AssignError> {
    assign_in_order(previous, ledger, release, objects).map(|(assigned, _)| assigned)
}

/// As [`assign`], with the id of each object in the order `objects` gave them.
///
/// # Errors
///
/// Fails as [`assign`] does.
pub fn assign_in_order(
    previous: &EntryIds,
    ledger: &mut Ledger,
    release: &str,
    objects: impl IntoIterator<Item = Vec<Designation>>,
) -> Result<(Assigned, Vec<ObjectId>), AssignError> {
    let (order, groups) = normalize(objects)?;
    for &id in previous.values() {
        if !ledger.is_issued(id) {
            return Err(AssignError::NotIssued(id));
        }
        if ledger.replacement(id).is_some() {
            return Err(AssignError::Replaced(id));
        }
    }

    // The previous objects each group carries entries of, and the groups each previous object's
    // entries went to.
    let carried: Vec<BTreeSet<ObjectId>> = groups
        .iter()
        .map(|g| g.iter().filter_map(|e| previous.get(e).copied()).collect())
        .collect();
    let mut went_to: BTreeMap<ObjectId, BTreeSet<usize>> =
        previous.values().map(|&id| (id, BTreeSet::new())).collect();
    for (g, ids) in carried.iter().enumerate() {
        for id in ids {
            went_to.entry(*id).or_default().insert(g);
        }
    }

    // Each group's id: the older of the previous objects it took whole, or a new one.
    let mut next = ledger.next_id()?;
    let mut issued: Option<IdRange> = None;
    let mut assigned = Vec::with_capacity(groups.len());
    for (g, entries) in groups.into_iter().enumerate() {
        let whole: Vec<ObjectId> = carried[g]
            .iter()
            .copied()
            .filter(|id| went_to[id].len() == 1)
            .collect();
        let (id, origin) = match whole.split_first() {
            Some((&oldest, [])) => (oldest, Origin::Kept),
            Some((&oldest, merged)) => (oldest, Origin::Merged(merged.to_vec())),
            None => {
                let id = next;
                next = ObjectId::new(id.get() + 1).map_err(|_| LedgerError::Exhausted)?;
                let first = issued.map_or(id, |range| range.first);
                issued = Some(IdRange { first, last: id });
                (id, Origin::New)
            }
        };
        assigned.push(AssignedObject {
            id,
            entries,
            origin,
        });
    }

    let replaced = replacements(&went_to, &assigned, release);
    ledger.record_release(release, issued, replaced.clone())?;
    let entries = assigned
        .iter()
        .flat_map(|o| o.entries.iter().map(|e| (e.clone(), o.id)))
        .collect();
    let mut ids = vec![None; assigned.len()];
    for (object, &given) in assigned.iter().zip(&order) {
        ids[given] = Some(object.id);
    }
    let assigned = Assigned {
        objects: assigned,
        entries,
        issued,
        replaced,
    };
    Ok((assigned, ids.into_iter().flatten().collect()))
}

/// Sorts each group's entries and the groups by their smallest entry, refusing an empty group or
/// an entry in two groups; with the groups, the place each had in the order given.
fn normalize(
    objects: impl IntoIterator<Item = Vec<Designation>>,
) -> Result<(Vec<usize>, Vec<Vec<Designation>>), AssignError> {
    let mut groups: Vec<(usize, Vec<Designation>)> = Vec::new();
    for (index, mut entries) in objects.into_iter().enumerate() {
        entries.sort();
        entries.dedup();
        if entries.is_empty() {
            return Err(AssignError::EmptyObject(index));
        }
        groups.push((index, entries));
    }
    let mut seen: HashSet<&Designation> = HashSet::new();
    for entry in groups.iter().flat_map(|(_, g)| g) {
        if !seen.insert(entry) {
            return Err(AssignError::EntryInTwoObjects(entry.clone()));
        }
    }
    groups.sort_by(|a, b| a.1[0].cmp(&b.1[0]));
    Ok(groups.into_iter().unzip())
}

/// What became of each previous id that does not go on: merged into the object that took it
/// whole, split into the objects its entries went to, or retired.
fn replacements(
    went_to: &BTreeMap<ObjectId, BTreeSet<usize>>,
    assigned: &[AssignedObject],
    release: &str,
) -> Vec<Replacement> {
    let mut replaced = Vec::new();
    for (&id, groups) in went_to {
        let mut by: Vec<ObjectId> = groups.iter().map(|&g| assigned[g].id).collect();
        by.sort();
        let change = match by.as_slice() {
            [] => Change::Retired,
            [one] if *one == id => continue,
            [_] => Change::MergedInto,
            _ => Change::SplitInto,
        };
        replaced.push(Replacement {
            id,
            release: release.to_owned(),
            change,
            replaced_by: by,
        });
    }
    replaced
}
