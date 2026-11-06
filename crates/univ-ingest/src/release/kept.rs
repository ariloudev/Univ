//! What a release numbers and places that the next release keeps: the number of each term,
//! source and transformation chain, and the cells and buckets of objects each object shard
//! holds. It is kept beside the id state (`numbering.yaml`, see [`crate::state`]), so that an
//! object, entry or source that does not change is written by the next release as the same rows
//! in the same database.
//!
//! A number once given is never given again, even when the next release no longer names its
//! term, source or chain.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::yaml::{FileError, read_checked, version_is, write};

/// The file, in a release's state directory, of what it numbers and places.
pub const NUMBERING: &str = "numbering.yaml";
/// The version of [`NUMBERING`] read and written here.
pub const NUMBERING_VERSION: u32 = 1;

/// A release's numbering and placement, as the next release keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    pub version: u32,
    /// Each term's id.
    pub terms: BTreeMap<String, u32>,
    /// Each source's index, `sources.n`, by its id.
    pub sources: BTreeMap<String, u32>,
    /// Each transformation chain's id, by its template's JSON.
    pub chains: BTreeMap<String, u32>,
    /// The Morton codes of the level-0 cells each object shard holds, shard by shard and scale
    /// domain by scale domain.
    pub cells: Vec<BTreeMap<String, Vec<u64>>>,
    /// The shard of each bucket of objects not placed that holds any, by bucket.
    pub loose: BTreeMap<u64, u32>,
}

impl Default for Kept {
    /// Nothing kept: before the first release.
    fn default() -> Self {
        Self {
            version: NUMBERING_VERSION,
            terms: BTreeMap::new(),
            sources: BTreeMap::new(),
            chains: BTreeMap::new(),
            cells: Vec::new(),
            loose: BTreeMap::new(),
        }
    }
}

impl Kept {
    /// What the release whose state is in `dir` kept, where its file is there.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, is not a numbering, is of another version, or gives
    /// one number to two terms, sources or chains.
    pub fn read(dir: &Path) -> Result<Option<Self>, FileError> {
        let file = dir.join(NUMBERING);
        if !file.exists() {
            return Ok(None);
        }
        read_checked(&file, |k: &Self| {
            version_is(|k: &Self| k.version, NUMBERING_VERSION)(k)?;
            for (field, numbers) in [
                ("terms", &k.terms),
                ("sources", &k.sources),
                ("chains", &k.chains),
            ] {
                let mut seen = BTreeMap::new();
                for (name, n) in numbers {
                    if let Some(first) = seen.insert(*n, name) {
                        return Err((
                            field.to_owned(),
                            format!("gives {n} to both {first:?} and {name:?}"),
                        ));
                    }
                }
            }
            Ok(())
        })
        .map(Some)
    }

    /// Writes it into `dir` through a rename, so that it is never half written.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        write(&dir.join(NUMBERING), self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numbering_is_kept_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Kept::read(dir.path()).unwrap(), None);
        let kept = Kept {
            terms: BTreeMap::from([("parsec".to_owned(), 0), ("galaxy".to_owned(), 4)]),
            sources: BTreeMap::from([("vizier-a".to_owned(), 1)]),
            chains: BTreeMap::from([("[]".to_owned(), 2)]),
            cells: vec![BTreeMap::from([("cosmic_web".to_owned(), vec![3, 9])])],
            loose: BTreeMap::from([(5, 0)]),
            ..Kept::default()
        };
        kept.write(dir.path()).unwrap();
        assert_eq!(Kept::read(dir.path()).unwrap(), Some(kept));
    }

    #[test]
    fn a_numbering_of_another_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let kept = Kept {
            version: 7,
            ..Kept::default()
        };
        kept.write(dir.path()).unwrap();
        let err = Kept::read(dir.path()).unwrap_err().to_string();
        assert!(err.contains("is 7, not 1"), "{err}");
    }

    #[test]
    fn a_numbering_that_gives_one_number_twice_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let kept = Kept {
            terms: BTreeMap::from([("galaxy".to_owned(), 4), ("parsec".to_owned(), 4)]),
            ..Kept::default()
        };
        kept.write(dir.path()).unwrap();
        let err = Kept::read(dir.path()).unwrap_err().to_string();
        assert!(
            err.contains(r#"gives 4 to both "galaxy" and "parsec""#),
            "{err}"
        );
    }
}
