//! Normalization: each row of a source read into a univ-model catalogue entry that keeps what
//! the source published, or left out of the release and counted under its reason. The entries
//! stay in memory for the next stage.

mod plan;
mod row;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::redshift::CmbCorrection;

use crate::source::{Access, LeftOutReason, SourceDefinition};
use crate::table::TableReader;
use plan::Plan;
use row::{Changed, Context};

/// A source normalized: its entries and what became of its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Normalized {
    pub source: SourceId,
    pub entries: Vec<CatalogueEntry>,
    pub report: NormalizationReport,
}

/// What normalization did with a source's rows, as its provenance records it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationReport {
    /// Entries published.
    pub published: u64,
    /// Entries left out, by reason. Every row is published or counted here.
    pub left_out: BTreeMap<LeftOutReason, u64>,
    /// Each fix of the definition that the normalizer applies, with the entries it changed.
    pub fixes: Vec<FixApplied>,
}

/// A fix applied, named by its change, with the number of entries it changed: entries
/// published with a value it changed, or left out by it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixApplied {
    pub change: String,
    pub entries_changed: u64,
}

impl NormalizationReport {
    /// The number of entries left out, for every reason.
    #[must_use]
    pub fn left_out_total(&self) -> u64 {
        self.left_out.values().sum()
    }
}

/// A source that cannot be normalized at all.
#[derive(Debug, Error, PartialEq)]
pub enum NormalizeError {
    #[error("{source_id}: {} cannot be read: {error}", file.display())]
    Read {
        source_id: SourceId,
        file: PathBuf,
        error: String,
    },
    #[error("{source_id}: {} is not a table: {error}", file.display())]
    Table {
        source_id: SourceId,
        file: PathBuf,
        error: String,
    },
    #[error("{source_id}: a table lacks the column {column}")]
    MissingColumn { source_id: SourceId, column: String },
    #[error("{source_id} cannot be normalized: {what}")]
    Unsupported { source_id: SourceId, what: String },
}

/// Whether the normalizer reads `definition`'s answers as tables of rows: not a source of
/// files, such as a grid, nor one kept in the repository.
#[must_use]
pub fn reads_rows(definition: &SourceDefinition) -> bool {
    !matches!(
        definition.access,
        Access::Files { .. } | Access::Repository { .. }
    )
}

/// Normalizes a source's tables one at a time, keeping the entries and the counts.
pub struct Normalizer<'a> {
    definition: &'a SourceDefinition,
    correction: &'a CmbCorrection,
    entries: Vec<CatalogueEntry>,
    left_out: BTreeMap<LeftOutReason, u64>,
    /// Entries each fix changed, by the fix's index in the definition.
    changed: Vec<u64>,
}

impl<'a> Normalizer<'a> {
    /// A normalizer of `definition`, moving redshifts to the CMB frame with `correction`.
    ///
    /// # Errors
    ///
    /// Fails for a source that is not a provider's table, such as one kept in the repository.
    pub fn new(
        definition: &'a SourceDefinition,
        correction: &'a CmbCorrection,
    ) -> Result<Self, NormalizeError> {
        if !reads_rows(definition) {
            return Err(NormalizeError::Unsupported {
                source_id: definition.id.clone(),
                what: "it is not a table of rows".to_owned(),
            });
        }
        Ok(Self {
            definition,
            correction,
            entries: Vec::new(),
            left_out: BTreeMap::new(),
            changed: vec![0; definition.fixes.len()],
        })
    }

    /// Reads one table, an answer to one chunk, named `file` in errors.
    ///
    /// # Errors
    ///
    /// Fails when the answer is not a table, a row has the wrong number of cells, or the table
    /// lacks a column the definition takes.
    pub fn table(&mut self, file: &Path, body: &[u8]) -> Result<(), NormalizeError> {
        let source_id = &self.definition.id;
        let malformed = |error: String| NormalizeError::Table {
            source_id: source_id.clone(),
            file: file.to_path_buf(),
            error,
        };
        let mut table = TableReader::new(&self.definition.access, body).map_err(malformed)?;
        if table.columns().is_empty() {
            return Ok(());
        }
        let header: Vec<&str> = table.columns().iter().map(Cow::as_ref).collect();
        let plan = Plan::new(self.definition, &header)?;
        let cx = Context {
            plan: &plan,
            source: source_id,
            epoch: self.definition.epoch,
            correction: self.correction,
        };
        while let Some(cells) = table.next_row() {
            let cells = cells.map_err(malformed)?;
            let changed = match row::read(&cx, cells) {
                Ok((entry, changed)) => {
                    self.entries.push(entry);
                    changed
                }
                Err((reason, changed)) => {
                    *self.left_out.entry(reason).or_default() += 1;
                    changed
                }
            };
            count(&mut self.changed, changed);
        }
        Ok(())
    }

    /// The entries and the report.
    #[must_use]
    pub fn finish(self) -> Normalized {
        let fixes = self
            .definition
            .fixes
            .iter()
            .zip(self.changed)
            .filter(|(fix, _)| fix.action.is_some())
            .map(|(fix, entries_changed)| FixApplied {
                change: fix.change.clone(),
                entries_changed,
            })
            .collect();
        let report = NormalizationReport {
            published: self.entries.len() as u64,
            left_out: self.left_out,
            fixes,
        };
        Normalized {
            source: self.definition.id.clone(),
            entries: self.entries,
            report,
        }
    }
}

/// Adds one to the count of each fix whose bit is set in `changed`.
fn count(counts: &mut [u64], changed: Changed) {
    for (i, n) in counts.iter_mut().enumerate() {
        if changed & (1 << i) != 0 {
            *n += 1;
        }
    }
}

/// Normalizes the tables in `files`, the chunks of `definition` on disk, in order.
///
/// # Errors
///
/// Fails when a file cannot be read or is not a table the definition can be read from.
pub fn normalize(
    definition: &SourceDefinition,
    files: &[PathBuf],
    correction: &CmbCorrection,
) -> Result<Normalized, NormalizeError> {
    let mut normalizer = Normalizer::new(definition, correction)?;
    for file in files {
        let body = std::fs::read(file).map_err(|e| NormalizeError::Read {
            source_id: definition.id.clone(),
            file: file.clone(),
            error: e.to_string(),
        })?;
        normalizer.table(file, &body)?;
    }
    Ok(normalizer.finish())
}
