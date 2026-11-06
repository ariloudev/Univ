//! What Univ Ingest keeps between releases, in its state directory (`--state`, by default
//! `.cache/univ-ingest/state`), as YAML:
//!
//! - `ledger.yaml`: the id ledger, every id issued and what became of each id replaced.
//! - `entry-ids.yaml`: the last release's map from each catalogue entry to its object's id.
//! - `numbering.yaml`: what the last release numbered and placed, kept by the next
//!   (`crate::release::Kept`); a build writes it, a cross-match never does.
//! - `pending/`: the same two files as a cross-match left them for the release it prepared. A
//!   cross-match builds no release, so it never touches the state itself, and a cross-match run
//!   again starts from the same state and gives the same ids. The build takes the state the
//!   cross-match made in memory, not this copy, and removes it once the release is whole.
//! - `univ-ingest.lock`: held by a match, tiles or build run for as long as it runs (see
//!   [`crate::lock`]).
//!
//! # One step makes a release exist
//!
//! A build writes each release's state whole into `states/{n}/` (the two files above), and then
//! switches `current`, a one-line file naming that directory, by writing `current.part` and
//! renaming it over `current`. The rename is the one step that records release n: before it the
//! state is release n − 1's, after it release n's. A state directory with no `current` holds the
//! two files itself, as one written before this layout did.
//!
//! Before the first release none of the files exists: the ledger is new, no entry has an id and
//! nothing is numbered. After it, they go together: a ledger that lists releases needs the entry
//! ids of the last, which must hold every id the ledger counts as live, issued and never
//! replaced, or the next release would give those objects new ids; and it needs what the last
//! release numbered and placed, or the next would number and place everything again.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use univ_model::ids::{Designation, ObjectId, SourceId};
use univ_model::ledger::Ledger;

use crate::ids::EntryIds;
use crate::release::ROWS;
use crate::release::{Kept, NUMBERING};
use crate::yaml::{FileError, read_checked, version_is, write};

/// The state directory a run uses when none is given.
pub const DEFAULT_STATE: &str = ".cache/univ-ingest/state";
/// The ledger's file in a state directory.
pub const LEDGER: &str = "ledger.yaml";
/// The entry-to-id map's file in a state directory.
pub const ENTRY_IDS: &str = "entry-ids.yaml";
/// The directory, within the state directory, of the state a cross-match prepared.
pub const PENDING: &str = "pending";
/// The file naming the directory, within the state directory, of the current state.
pub const CURRENT: &str = "current";
/// The directory, within the state directory, of each release's state.
pub const STATES: &str = "states";
/// The version of the entry-to-id file read and written here.
pub const ENTRY_IDS_VERSION: u32 = 1;

/// The ids of the last release and the ledger they are recorded in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub ledger: Ledger,
    /// Each catalogue entry of the last release with its object's id; empty before the first.
    pub previous: EntryIds,
    /// What the last release numbered and placed; nothing before the first.
    pub kept: Kept,
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

/// The entry ids as their file holds them, written from the map without a copy of it.
#[derive(Serialize)]
struct EntryIdsOut<'a> {
    version: u32,
    entries: EntriesOut<'a>,
}

/// The rows of the entry-id map, one per entry, in its order (sorted).
struct EntriesOut<'a>(&'a EntryIds);

/// One row of [`EntriesOut`].
#[derive(Serialize)]
struct EntryIdOut<'a> {
    source: &'a SourceId,
    identifier: &'a str,
    id: ObjectId,
}

impl Serialize for EntriesOut<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|(entry, id)| EntryIdOut {
            source: &entry.source,
            identifier: &entry.identifier,
            id: *id,
        }))
    }
}

impl State {
    /// The state kept in `dir`: a new ledger and no ids where neither file exists.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read or does not stand, or when the entry ids are kept
    /// without the ledger that issued them.
    pub fn read(dir: &Path) -> Result<Self, FileError> {
        Self::read_files(&current_dir(dir)?)
    }

    /// The state whose two files are in `dir` itself.
    fn read_files(dir: &Path) -> Result<Self, FileError> {
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
        let kept = match Kept::read(dir)? {
            Some(kept) => kept,
            None => match ledger.releases().last() {
                Some(last) => {
                    return Err(FileError::new(
                        &dir.join(NUMBERING),
                        "(file)",
                        format!(
                            "is missing, but the ledger lists {} release(s), the last {:?}: \
                             without what that release numbered and placed, every term, source \
                             and chain would be numbered and every object placed again",
                            ledger.releases().len(),
                            last.name
                        ),
                    ));
                }
                None => Kept::default(),
            },
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
        Ok(Self {
            ledger,
            previous,
            kept,
        })
    }

    /// Writes the state to `dir`, creating it, each file through a rename so that none is
    /// ever half written.
    ///
    /// # Errors
    ///
    /// Fails when the directory or a file cannot be written.
    pub fn write(&self, dir: &Path) -> io::Result<()> {
        self.write_with(dir, &self.kept)
    }

    /// [`Self::write`], with `kept` in place of its own.
    fn write_with(&self, dir: &Path, kept: &Kept) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        kept.write(dir)?;
        write_ledger(&dir.join(LEDGER), &self.ledger)?;
        let file = EntryIdsOut {
            version: ENTRY_IDS_VERSION,
            entries: EntriesOut(&self.previous),
        };
        write(&dir.join(ENTRY_IDS), &file)
    }
}

impl State {
    /// Makes this the state of the state directory `dir` in one step: writes it whole into
    /// `states/{n}/`, n the releases its ledger records, with `kept`, what the release
    /// numbered and placed, in place of its own, and `rows`, the release's row index, moved
    /// into it; then renames `current.part`, naming that directory, over `current`. The row
    /// index of the state it replaces, which no build compares with again, is removed.
    ///
    /// # Errors
    ///
    /// Fails, leaving the state of `dir` as it was, when a directory or file cannot be written.
    pub fn install(&self, dir: &Path, kept: &Kept, rows: &Path) -> io::Result<()> {
        let name = format!("{STATES}/{}", self.ledger.releases().len());
        let target = dir.join(&name);
        let part = dir.join(format!("{name}.part"));
        if part.exists() {
            std::fs::remove_dir_all(&part)?;
        }
        let replaced = rows_dir(dir).ok().flatten();
        self.write_with(&part, kept)?;
        std::fs::rename(rows, part.join(ROWS))?;
        // A directory of this number that `current` does not name was left by a build that
        // stopped before it switched `current`.
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&part, &target)?;
        let switched = crate::fetch::write_via_part(
            &dir.join(CURRENT),
            format!("{name}\n").as_bytes(),
            || Ok(()),
        );
        if switched.is_err() {
            let _ = std::fs::remove_dir_all(&target);
        } else if let Some(replaced) = replaced.filter(|r| !r.starts_with(&target)) {
            let _ = std::fs::remove_dir_all(replaced);
        }
        switched
    }

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

/// The directory, within the state directory `dir`, of its current state: the one `current`
/// names, or `dir` itself for a state written before `current` was.
///
/// # Errors
///
/// Fails when `current` cannot be read or names no directory `states/{n}`.
fn current_dir(dir: &Path) -> Result<PathBuf, FileError> {
    let current = dir.join(CURRENT);
    if !current.exists() {
        return Ok(dir.to_path_buf());
    }
    let named = std::fs::read_to_string(&current)
        .map_err(|e| FileError::new(&current, "(file)", e.to_string()))?;
    let named = named.trim();
    let valid = named
        .strip_prefix(&format!("{STATES}/"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return Err(FileError::new(
            &current,
            "(file)",
            format!("names {named:?}, not a directory {STATES}/{{n}}"),
        ));
    }
    Ok(dir.join(named))
}

/// The row index of the state of the state directory `dir`, which the next build compares its
/// rows with: none before the first release, or for a state written before row indexes were.
///
/// # Errors
///
/// Fails when `current` cannot be read.
pub fn rows_dir(dir: &Path) -> Result<Option<PathBuf>, FileError> {
    let rows = current_dir(dir)?.join(ROWS);
    Ok(rows.is_dir().then_some(rows))
}

/// Where a build writes the row index of the release it makes, within the state directory
/// `dir`, until the state is installed with it.
#[must_use]
pub fn next_rows_dir(dir: &Path) -> PathBuf {
    dir.join(format!("{ROWS}.part"))
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
    let read: EntryIdsFile = read_checked(
        file,
        version_is(|f: &EntryIdsFile| f.version, ENTRY_IDS_VERSION),
    )?;
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
