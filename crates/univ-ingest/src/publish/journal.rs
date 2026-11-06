//! The publish journal: what of release n is already in local D1 and R2, uploaded and checked,
//! so that an interrupted publish continues where it stopped (AC-105).
//!
//! It is YAML at `publish-{n}.yaml` in the journal directory, written whole through a rename
//! after each unit is checked, so a journal never lists a unit that was not:
//!
//! ```yaml
//! format: univ-publish-journal
//! release: 2
//! manifest_sha256: 9f2c…
//! started_at: 2026-11-03T12:00:00Z
//! done:
//! - path: d1/catalogue/0000-schema.sql
//!   target: d1
//!   sha256: 41ab…
//!   checked: schema version 5 in the catalogue database
//!   at: 2026-11-03T12:00:01Z
//! - path: rules/matching.yaml
//!   target: r2
//!   key: releases/2/rules/matching.yaml
//!   sha256: 0c7e…
//!   checked: read back with the manifest's SHA-256
//!   at: 2026-11-03T12:00:02Z
//! latest_moved_at: 2026-11-03T12:00:09Z
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use univ_model::release::manifest::Target;

use crate::yaml::{FileError, read_checked, write};

/// The name a publish journal gives its format.
pub const FORMAT: &str = "univ-publish-journal";

/// A publish journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub format: String,
    pub release: u32,
    /// The SHA-256 of the release manifest the journal is of.
    pub manifest_sha256: String,
    /// RFC 3339, UTC.
    pub started_at: String,
    /// Each file uploaded and checked, in the order it was.
    pub done: Vec<Done>,
    /// When `latest` was moved to the release, once it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_moved_at: Option<String>,
}

/// A file uploaded and checked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Done {
    /// Its path in the release directory.
    pub path: String,
    pub target: Target,
    /// Its R2 key, for a file uploaded to R2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub sha256: String,
    /// What was checked once it was uploaded.
    pub checked: String,
    /// RFC 3339, UTC.
    pub at: String,
}

/// The journal file of release `number` in the journal directory `dir`.
#[must_use]
pub fn file(dir: &Path, number: u32) -> PathBuf {
    dir.join(format!("publish-{number}.yaml"))
}

impl Journal {
    /// A new journal of release `number` under the manifest with SHA-256 `manifest_sha256`.
    #[must_use]
    pub fn new(number: u32, manifest_sha256: &str, started_at: &str) -> Self {
        Self {
            format: FORMAT.to_owned(),
            release: number,
            manifest_sha256: manifest_sha256.to_owned(),
            started_at: started_at.to_owned(),
            done: Vec::new(),
            latest_moved_at: None,
        }
    }

    /// Reads the journal of release `number` from `dir`: none where there is none.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read or is not the journal of release `number`.
    pub fn read(dir: &Path, number: u32) -> Result<Option<Self>, FileError> {
        let path = file(dir, number);
        if !path.exists() {
            return Ok(None);
        }
        read_checked(&path, |j: &Self| {
            if j.format != FORMAT {
                return Err(("format".into(), format!("not {FORMAT}")));
            }
            if j.release != number {
                return Err(("release".into(), format!("not {number}")));
            }
            Ok(())
        })
        .map(Some)
    }

    /// Writes the journal to `dir`, whole.
    ///
    /// # Errors
    ///
    /// Fails when the directory or file cannot be written.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        write(&file(dir, self.release), self)
    }

    /// Whether the file at `path` with SHA-256 `sha256` was uploaded and checked.
    #[must_use]
    pub fn has(&self, path: &str, sha256: &str) -> bool {
        self.done
            .iter()
            .any(|d| d.path == path && d.sha256 == sha256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_journal_reads_back_as_written_and_refuses_another_release() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Journal::read(dir.path(), 3).unwrap(), None);
        let mut journal = Journal::new(3, "abc", "2026-11-03T12:00:00Z");
        journal.done.push(Done {
            path: "rules/matching.yaml".into(),
            target: Target::R2,
            key: Some("releases/3/rules/matching.yaml".into()),
            sha256: "def".into(),
            checked: "read back".into(),
            at: "2026-11-03T12:00:01Z".into(),
        });
        journal.write(dir.path()).unwrap();
        let back = Journal::read(dir.path(), 3).unwrap().unwrap();
        assert_eq!(back, journal);
        assert!(back.has("rules/matching.yaml", "def"));
        assert!(!back.has("rules/matching.yaml", "other"));
        std::fs::rename(file(dir.path(), 3), file(dir.path(), 4)).unwrap();
        let refused = Journal::read(dir.path(), 4).unwrap_err();
        assert_eq!(refused.field, "release");
    }
}
