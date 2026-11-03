//! YAML files read with their checks, such as source definitions and structure files, and
//! written whole. A file that cannot be used is refused naming the file, the field and what is
//! wrong.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

/// A YAML file that cannot be used, naming the file, the field and what is wrong.
#[derive(Debug, Error, PartialEq)]
#[error("{}: {field}: {problem}", file.display())]
pub struct FileError {
    pub file: PathBuf,
    pub field: String,
    pub problem: String,
}

impl FileError {
    /// The refusal of `file` for `problem` in `field`.
    #[must_use]
    pub fn new(file: &Path, field: &str, problem: impl Into<String>) -> Self {
        Self {
            file: file.to_path_buf(),
            field: field.to_owned(),
            problem: problem.into(),
        }
    }
}

/// Reads `file` as a `T`, then makes the checks serde cannot, which give the field and the
/// problem.
///
/// # Errors
///
/// Fails when the file cannot be read, its YAML does not parse as a `T`, naming the field, or
/// `check` refuses it.
pub fn read_checked<T: DeserializeOwned>(
    file: &Path,
    check: impl FnOnce(&T) -> Result<(), (String, String)>,
) -> Result<T, FileError> {
    let text =
        fs::read_to_string(file).map_err(|e| FileError::new(file, "(file)", e.to_string()))?;
    parse_checked(file, &text, check)
}

/// Parses `text`, read from `file` (a path or another name of where it came from, such as an R2
/// key), as a `T`, then makes the checks serde cannot, as [`read_checked`] does.
///
/// # Errors
///
/// Fails when the YAML does not parse as a `T`, naming the field, or `check` refuses it.
pub fn parse_checked<T: DeserializeOwned>(
    file: &Path,
    text: &str,
    check: impl FnOnce(&T) -> Result<(), (String, String)>,
) -> Result<T, FileError> {
    let document = serde_norway::Deserializer::from_str(text);
    let value: T = serde_path_to_error::deserialize(document).map_err(|e| {
        let path = e.path().to_string();
        // The YAML error may already lead with the field; say it once.
        let message = e.into_inner().to_string();
        let problem = message
            .strip_prefix(&format!("{path}: "))
            .unwrap_or(&message);
        let field = if path == "." { "(top level)" } else { &path };
        FileError::new(file, field, problem)
    })?;
    check(&value).map_err(|(field, problem)| FileError::new(file, &field, problem))?;
    Ok(value)
}

/// Writes `value` to `file` as YAML through a rename, so that it is never half written.
///
/// # Errors
///
/// Fails when the value cannot be written as YAML or the file cannot be written.
pub fn write<T: Serialize>(file: &Path, value: &T) -> std::io::Result<()> {
    let yaml = serde_norway::to_string(value).map_err(std::io::Error::other)?;
    crate::fetch::write_via_part(file, yaml.as_bytes(), || Ok(()))
}

/// The `.yaml` files in `dir`, in file-name order.
///
/// # Errors
///
/// Fails when the directory cannot be read.
pub fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, FileError> {
    let entries =
        fs::read_dir(dir).map_err(|e| FileError::new(dir, "(directory)", e.to_string()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    files.sort();
    Ok(files)
}
