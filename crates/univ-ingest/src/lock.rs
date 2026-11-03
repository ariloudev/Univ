//! The lock of a state directory: one match, tiles or build run at a time, since each reads the
//! state and a build changes it.
//!
//! The lock is the file `univ-ingest.lock` in the state directory, created only where it does
//! not exist, holding the process id and start time of the run that holds it, and removed when
//! the run ends. A run that finds it refuses, naming the run that holds it. A lock left by a run
//! that ended without removing it, such as one killed, is never taken over: the refusal says how
//! to remove it once no run is going on.

use std::fmt;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The lock's file in a state directory.
pub const FILE: &str = "univ-ingest.lock";

/// What the lock file holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holder {
    pub pid: u32,
    /// RFC 3339, UTC.
    pub started_at: String,
}

/// The lock of a state directory, held until it is dropped.
#[derive(Debug)]
pub struct StateLock {
    file: PathBuf,
}

/// A lock that could not be taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockError {
    /// Another run holds it, or a run that ended left it.
    Held {
        file: PathBuf,
        holder: Option<Holder>,
    },
    /// The state directory or the lock file cannot be written.
    Io { file: PathBuf, problem: String },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Held { file, holder } => {
                let by = holder.as_ref().map_or_else(
                    || "a run whose lock file cannot be read".to_owned(),
                    |h| format!("process {} since {}", h.pid, h.started_at),
                );
                write!(
                    f,
                    "the state directory is locked by {by} ({}): one match, tiles or build runs \
                     at a time. If no univ-ingest run is going on (that process has ended), the \
                     lock was left by a run that stopped without removing it: remove {} and run \
                     again",
                    file.display(),
                    file.display()
                )
            }
            Self::Io { file, problem } => {
                write!(f, "the lock {} cannot be taken: {problem}", file.display())
            }
        }
    }
}

impl std::error::Error for LockError {}

impl StateLock {
    /// Takes the lock of the state directory `dir`, creating the directory, for the process
    /// `pid` started at `started_at`.
    ///
    /// # Errors
    ///
    /// Refuses when the lock file exists, naming what it holds; fails when it cannot be written.
    pub fn take(dir: &Path, pid: u32, started_at: &str) -> Result<Self, LockError> {
        let file = dir.join(FILE);
        let io = |file: &Path, e: &dyn fmt::Display| LockError::Io {
            file: file.to_path_buf(),
            problem: e.to_string(),
        };
        std::fs::create_dir_all(dir).map_err(|e| io(&file, &e))?;
        let mut created = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(created) => created,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let holder = std::fs::read_to_string(&file)
                    .ok()
                    .and_then(|t| serde_norway::from_str(&t).ok());
                return Err(LockError::Held { file, holder });
            }
            Err(e) => return Err(io(&file, &e)),
        };
        let lock = Self { file };
        let holder = Holder {
            pid,
            started_at: started_at.to_owned(),
        };
        let yaml = serde_norway::to_string(&holder).map_err(|e| io(&lock.file, &e))?;
        created
            .write_all(yaml.as_bytes())
            .map_err(|e| io(&lock.file, &e))?;
        Ok(lock)
    }

    /// The lock file.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_lock_is_refused_naming_its_holder_and_never_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let lock = StateLock::take(dir.path(), 41, "2026-11-03T12:00:00Z").unwrap();
        let refused = StateLock::take(dir.path(), 42, "2026-11-03T12:00:01Z").unwrap_err();
        let LockError::Held { holder, .. } = &refused else {
            panic!("{refused}")
        };
        assert_eq!(holder.as_ref().unwrap().pid, 41);
        let message = refused.to_string();
        assert!(message.contains("locked by process 41 since 2026-11-03T12:00:00Z"));
        assert!(message.contains(&format!("remove {}", lock.file().display())));
        drop(lock);
        assert!(!dir.path().join(FILE).exists());
        // A lock a killed run left stays until it is removed.
        std::fs::write(dir.path().join(FILE), "pid: 7\nstarted_at: then\n").unwrap();
        assert!(StateLock::take(dir.path(), 43, "now").is_err());
        assert!(dir.path().join(FILE).exists());
    }
}
