//! What Univ Ingest keeps between releases, in its state directory (`--state`, by default
//! `.cache/univ-ingest/state`), as YAML:
//!
//! - `ledger.yaml`: the id ledger, every id issued and what became of each id replaced.
//! - `entry-ids.yaml`: the last release's map from each catalogue entry to its object's id.
//! - `pending/`: the same two files as a cross-match left them for the release it prepared. A
//!   cross-match builds no release, so it never touches the files above: the release build takes
//!   `pending/` as the new state once the release is whole, and a cross-match run again starts
//!   from the same state and gives the same ids.
//!
//! Before the first release neither file exists: the ledger is new and no entry has an id. After
//! it, the two go together: a ledger that lists releases needs the entry ids of the last, and
//! those must hold every id the ledger counts as live, issued and never replaced, or the next
//! release would give those objects new ids.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use univ_model::ids::{Designation, ObjectId, SourceId};
use univ_model::ledger::Ledger;

use crate::ids::EntryIds;
use crate::yaml::{FileError, read_checked, write};

/// The state directory a run uses when none is given.
pub const DEFAULT_STATE: &str = ".cache/univ-ingest/state";
/// The ledger's file in a state directory.
pub const LEDGER: &str = "ledger.yaml";
/// The entry-to-id map's file in a state directory.
pub const ENTRY_IDS: &str = "entry-ids.yaml";
/// The directory, within the state directory, of the state a cross-match prepared.
pub const PENDING: &str = "pending";
/// The version of the entry-to-id file read and written here.
pub const ENTRY_IDS_VERSION: u32 = 1;

/// The ids of the last release and the ledger they are recorded in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub ledger: Ledger,
    /// Each catalogue entry of the last release with its object's id; empty before the first.
    pub previous: EntryIds,
}

/// The entry-to-id map as its file holds it: one row per entry, sorted.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryIdsFile {
    version: u32,
    entries: Vec<EntryId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryId {
    source: SourceId,
    identifier: String,
    id: ObjectId,
}

impl State {
    /// The state kept in `dir`: a new ledger and no ids where neither file exists.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read or does not stand, or when the entry ids are kept
    /// without the ledger that issued them.
    pub fn read(dir: &Path) -> Result<Self, FileError> {
        let (ledger, ids) = (dir.join(LEDGER), dir.join(ENTRY_IDS));
        let ledger = if ledger.exists() {
            read_ledger(&ledger)?
        } else if ids.exists() {
            return Err(FileError::new(
                &ledger,
                "(file)",
                format!(
                    "is missing beside {ENTRY_IDS}: ids cannot be checked against a new ledger"
                ),
            ));
        } else {
            Ledger::new()
        };
        let previous = if ids.exists() {
            read_entry_ids(&ids)?
        } else if let Some(last) = ledger.releases().last() {
            return Err(FileError::new(
                &ids,
                "(file)",
                format!(
                    "is missing, but the ledger lists {} release(s), the last {:?}: without the \
                     entry ids of that release every object would be given a new id",
                    ledger.releases().len(),
                    last.name
                ),
            ));
        } else {
            EntryIds::new()
        };
        let missing = uncovered(&ledger, &previous);
        if let Some(first) = missing.first() {
            let shown: Vec<String> = missing.iter().take(10).map(ToString::to_string).collect();
            return Err(FileError::new(
                &ids,
                "entries",
                format!(
                    "holds no entry for {} id(s) the ledger counts as live (issued and never \
                     replaced), from {first}: {}{}",
                    missing.len(),
                    shown.join(", "),
                    if missing.len() > 10 { ", ..." } else { "" }
                ),
            ));
        }
        Ok(Self { ledger, previous })
    }

    /// Writes the state to `dir`, creating it, each file through a rename so that none is
    /// ever half written.
    ///
    /// # Errors
    ///
    /// Fails when the directory or a file cannot be written.
    pub fn write(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        write_ledger(&dir.join(LEDGER), &self.ledger)?;
        let file = EntryIdsFile {
            version: ENTRY_IDS_VERSION,
            entries: self
                .previous
                .iter()
                .map(|(entry, id)| EntryId {
                    source: entry.source.clone(),
                    identifier: entry.identifier.clone(),
                    id: *id,
                })
                .collect(),
        };
        write(&dir.join(ENTRY_IDS), &file)
    }
}

impl State {
    /// Writes the state as the one a cross-match prepared in the state directory `dir`: into
    /// `pending/`, whole, through a directory written beside it and renamed into place.
    ///
    /// # Errors
    ///
    /// Fails when a directory or a file cannot be written.
    pub fn write_pending(&self, dir: &Path) -> io::Result<()> {
        let pending = pending(dir);
        let part = dir.join(format!("{PENDING}.part"));
        if part.exists() {
            std::fs::remove_dir_all(&part)?;
        }
        self.write(&part)?;
        if pending.exists() {
            std::fs::remove_dir_all(&pending)?;
        }
        std::fs::rename(part, pending)
    }
}

/// The directory of the state a cross-match prepared, within `dir`.
#[must_use]
pub fn pending(dir: &Path) -> PathBuf {
    dir.join(PENDING)
}

/// The ids `ledger` counts as live, issued and never replaced, that no entry of `previous` holds,
/// ascending.
fn uncovered(ledger: &Ledger, previous: &EntryIds) -> Vec<ObjectId> {
    let Some(last) = ledger.last_issued() else {
        return Vec::new();
    };
    let held: HashSet<ObjectId> = previous.values().copied().collect();
    let replaced: HashSet<ObjectId> = ledger.replaced().iter().map(|r| r.id).collect();
    (1..=last.get())
        .filter_map(|n| ObjectId::new(n).ok())
        .filter(|id| !held.contains(id) && !replaced.contains(id))
        .collect()
}

/// Reads the id ledger kept in `file`, with the ledger's checks.
///
/// A missing file is refused rather than taken for an empty ledger: starting over would give
/// ids again. The first release starts from [`Ledger::new`].
///
/// # Errors
///
/// Fails when the file cannot be read, its YAML is not a ledger, or the ledger does not stand,
/// naming the file and the field.
pub fn read_ledger(file: &Path) -> Result<Ledger, FileError> {
    read_checked(file, |_: &Ledger| Ok(()))
}

/// Writes `ledger` to `file` through a rename, so that it is never half written.
///
/// # Errors
///
/// Fails when the file cannot be written.
pub fn write_ledger(file: &Path, ledger: &Ledger) -> io::Result<()> {
    write(file, ledger)
}

fn read_entry_ids(file: &Path) -> Result<EntryIds, FileError> {
    let read: EntryIdsFile = read_checked(file, |f: &EntryIdsFile| {
        if f.version == ENTRY_IDS_VERSION {
            Ok(())
        } else {
            Err((
                "version".to_owned(),
                format!("is {}, not {ENTRY_IDS_VERSION}", f.version),
            ))
        }
    })?;
    let mut ids = EntryIds::new();
    for (i, row) in read.entries.into_iter().enumerate() {
        let field = format!("entries[{i}]");
        let entry = Designation::new(row.source, row.identifier)
            .map_err(|e| FileError::new(file, &field, e.to_string()))?;
        if ids.insert(entry, row.id).is_some() {
            return Err(FileError::new(file, &field, "repeats an entry"));
        }
    }
    Ok(ids)
}
