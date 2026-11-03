//! The id ledger's YAML file.

use std::io;
use std::path::Path;

use univ_model::ledger::Ledger;

use crate::fetch::write_via_part;
use crate::yaml::{FileError, read_checked};

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
    let yaml = serde_norway::to_string(ledger).map_err(io::Error::other)?;
    write_via_part(file, yaml.as_bytes(), || Ok(()))
}
