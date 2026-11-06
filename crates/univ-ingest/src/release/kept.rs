//! What a release numbers and places that the next release keeps: the number of each term,
//! source and transformation chain, and the cells and buckets of objects each object shard
//! holds. It is kept beside the id state (`numbering.yaml`), so that an object, entry or source
//! that does not change is written by the next release as the same rows in the same database.
//!
//! A number once given is never given again, even when the next release no longer names its
//! term, source or chain.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::yaml::{FileError, read_checked, write};

/// The file, in a release's state directory, of what it numbers and places.
pub const NUMBERING: &str = "numbering.yaml";
/// The version of [`NUMBERING`] read and written here.
pub const NUMBERING_VERSION: u32 = 1;

/// A release's numbering and placement, as the next release keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    pub version: u32,
    /// Each term's id.
    pub terms: BTreeMap<String, u32>,
    /// Each source's index, `sources.n`, by its id.
    pub sources: BTreeMap<String, u32>,
    /// Each transformation chain's id, by its template's JSON.
    pub chains: BTreeMap<String, u32>,
    /// The runs of level-0 cells each object shard holds, shard by shard.
    pub shards: Vec<Vec<KeptRun>>,
    /// The shard of each bucket of objects not placed that holds any, by bucket.
    pub loose: BTreeMap<u64, u32>,
}

/// A run of cells of one scale domain that an object shard holds, by their Morton codes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptRun {
    pub domain: String,
    pub first: u64,
    pub last: u64,
}

impl Kept {
    /// What the release whose state is in `dir` kept: nothing where it kept no numbering, as
    /// before the first release or for a state written before numbering was kept.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, is not a numbering, or is of another version.
    pub fn read(dir: &Path) -> Result<Self, FileError> {
        let file = dir.join(NUMBERING);
        if !file.exists() {
            return Ok(Self::default());
        }
        read_checked(&file, |k: &Self| {
            if k.version == NUMBERING_VERSION {
                Ok(())
            } else {
                Err((
                    "version".to_owned(),
                    format!("is {}, not {NUMBERING_VERSION}", k.version),
                ))
            }
        })
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

/// The next number after every number of `given`: numbers are never given twice.
#[must_use]
pub fn next_after<'a>(given: impl IntoIterator<Item = &'a u32>) -> u32 {
    given.into_iter().max().map_or(0, |m| m.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numbering_is_kept_and_read_back_and_none_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Kept::read(dir.path()).unwrap(), Kept::default());
        let kept = Kept {
            version: NUMBERING_VERSION,
            terms: BTreeMap::from([("parsec".to_owned(), 0), ("galaxy".to_owned(), 4)]),
            sources: BTreeMap::from([("vizier-a".to_owned(), 1)]),
            chains: BTreeMap::from([("[]".to_owned(), 2)]),
            shards: vec![vec![KeptRun {
                domain: "cosmic_web".to_owned(),
                first: 3,
                last: 9,
            }]],
            loose: BTreeMap::from([(5, 0)]),
        };
        kept.write(dir.path()).unwrap();
        assert_eq!(Kept::read(dir.path()).unwrap(), kept);
        assert_eq!(next_after(kept.terms.values()), 5);
        assert_eq!(next_after(&[]), 0);
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
}
