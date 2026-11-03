//! The release configuration, `rules/release.yaml`: the release cosmology, Planck 2018 unless
//! the file names another.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::cosmology::Cosmology;

use crate::yaml::{FileError, read_checked};

/// The release configuration's file in the repository's `rules/`.
pub const FILE: &str = "release.yaml";

/// What a release is built under beyond its rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseConfig {
    /// The cosmology every derived distance and position of the release is placed under.
    pub cosmology: Cosmology,
    /// Why, with its citation.
    pub cosmology_basis: String,
}

impl Default for ReleaseConfig {
    /// Planck 2018, the cosmology of every release that names no other.
    fn default() -> Self {
        Self {
            cosmology: Cosmology::planck_2018(),
            cosmology_basis: "No release configuration was given: Planck 2018 (Planck \
                              Collaboration 2020, A&A 641, A6, Table 2), the default."
                .to_owned(),
        }
    }
}

impl ReleaseConfig {
    /// The configuration in `file`, or the default when there is none.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, when the file exists but cannot be read, does not
    /// parse, names a cosmology univ-model refuses, or gives no basis.
    pub fn read(file: &Path) -> Result<Self, FileError> {
        if !file.exists() {
            return Ok(Self::default());
        }
        read_checked(file, |config: &Self| {
            if config.cosmology_basis.trim().is_empty() {
                return Err(("cosmology_basis".to_owned(), "is empty".to_owned()));
            }
            Ok(())
        })
    }
}
