//! Normalization: each row of a source read into a univ-model catalogue entry that keeps what
//! the source published, or left out of the release and counted under its reason. The entries
//! stay in memory for the next stage.

mod bintable;
mod csv_file;
mod plan;
mod row;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::redshift::CmbCorrection;

use crate::source::{Access, Field, Holds, LeftOutReason, RemoteFile, SourceDefinition};
use crate::table::TableReader;
use bintable::BinTable;
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
    #[error("{source_id}: normalization was cancelled")]
    Cancelled { source_id: SourceId },
}

impl NormalizeError {
    fn read(source_id: &SourceId, file: &Path, error: &dyn std::fmt::Display) -> Self {
        Self::Read {
            source_id: source_id.clone(),
            file: file.to_path_buf(),
            error: error.to_string(),
        }
    }

    fn table(source_id: &SourceId, file: &Path, error: String) -> Self {
        Self::Table {
            source_id: source_id.clone(),
            file: file.to_path_buf(),
            error,
        }
    }
}

/// Rows read between two looks at the cancel flag.
const CANCEL_EVERY: usize = 4096;

/// What the files of a files source hold, when they are tables of one kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileTables {
    /// FITS binary tables, read whole and joined.
    Fits,
    /// Gzipped CSV tables, read a piece at a time.
    CsvGz,
}

/// The kind of table every one of `files` holds; none when they are not all tables of one kind.
fn file_tables(files: &[RemoteFile]) -> Option<FileTables> {
    let all = |is: fn(&str) -> bool| files.iter().all(|f| is(&f.path));
    if all(crate::fits::is_fits) {
        Some(FileTables::Fits)
    } else if all(csv_file::is_csv_gz) {
        Some(FileTables::CsvGz)
    } else {
        None
    }
}

/// Whether normalization gives `definition` entries: a provider's answers, files that are all
/// FITS tables or all gzipped CSV tables, or the researched star distances, but not grids nor
/// the researched structures.
#[must_use]
pub fn reads_rows(definition: &SourceDefinition) -> bool {
    match &definition.access {
        Access::Files { files } => file_tables(files).is_some(),
        Access::Repository { holds, .. } => *holds == Holds::StarDistances,
        _ => true,
    }
}

/// Normalizes a source's tables one at a time, keeping the entries and the counts.
pub struct Normalizer<'a> {
    definition: &'a SourceDefinition,
    correction: &'a CmbCorrection,
    entries: Vec<CatalogueEntry>,
    /// The identifiers of the entries kept so far.
    identifiers: HashSet<String>,
    left_out: BTreeMap<LeftOutReason, u64>,
    /// Entries each fix changed, by the fix's index in the definition.
    changed: Vec<u64>,
    /// Set by the owner's interrupt: the normalizer then stops at its next look.
    cancel: Option<&'a AtomicBool>,
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
            identifiers: HashSet::new(),
            left_out: BTreeMap::new(),
            changed: vec![0; definition.fixes.len()],
            cancel: None,
        })
    }

    /// The normalizer, stopping with [`NormalizeError::Cancelled`] once `flag` is set: it looks
    /// before each table and every [`CANCEL_EVERY`] rows.
    #[must_use]
    pub fn cancelled_by(mut self, flag: &'a AtomicBool) -> Self {
        self.cancel = Some(flag);
        self
    }

    /// Fails once the cancel flag is set, looking at row `row` and every [`CANCEL_EVERY`] rows.
    fn go_on(&self, row: usize) -> Result<(), NormalizeError> {
        if row.is_multiple_of(CANCEL_EVERY)
            && self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(NormalizeError::Cancelled {
                source_id: self.definition.id.clone(),
            });
        }
        Ok(())
    }

    /// Reads one table, an answer to one chunk, named `file` in errors.
    ///
    /// # Errors
    ///
    /// Fails when the answer is not a table, a row has the wrong number of cells, or the table
    /// lacks a column the definition takes.
    pub fn table(&mut self, file: &Path, body: &[u8]) -> Result<(), NormalizeError> {
        let table = TableReader::new(&self.definition.access, body)
            .map_err(|e| NormalizeError::table(&self.definition.id, file, e))?;
        self.rows(file, table)
    }

    /// Reads the rows of `table`, from the file `file`.
    fn rows(&mut self, file: &Path, mut table: TableReader<'_>) -> Result<(), NormalizeError> {
        self.go_on(0)?;
        let source_id = &self.definition.id;
        let malformed = |error: String| NormalizeError::table(source_id, file, error);
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
        let mut n = 0;
        while let Some(cells) = table.next_row() {
            n += 1;
            self.go_on(n)?;
            self.row(&cx, cells.map_err(malformed)?);
        }
        Ok(())
    }

    /// Reads one row: an entry kept, or a reason counted, such as a row of blank cells.
    fn row(&mut self, cx: &Context<'_>, cells: &[Cow<'_, str>]) {
        if cells.iter().all(|c| c.is_empty()) {
            *self.left_out.entry(LeftOutReason::BlankRow).or_default() += 1;
            return;
        }
        let changed = match row::read(cx, cells) {
            Ok((entry, changed)) => {
                if self
                    .identifiers
                    .insert(entry.designation.identifier.clone())
                {
                    self.entries.push(entry);
                } else {
                    let reason = LeftOutReason::DuplicateIdentifier;
                    *self.left_out.entry(reason).or_default() += 1;
                }
                changed
            }
            Err((reason, changed)) => {
                *self.left_out.entry(reason).or_default() += 1;
                changed
            }
        };
        count(&mut self.changed, changed);
    }

    /// Reads FITS tables as one: each row of the first, joined on the identifier to the rows of
    /// the others, each column taken from the first table that has it. A row the others lack
    /// takes their columns as blank.
    fn fits(&mut self, tables: &[(PathBuf, BinTable)]) -> Result<(), NormalizeError> {
        let definition = self.definition;
        let unsupported = |what: String| NormalizeError::Unsupported {
            source_id: definition.id.clone(),
            what,
        };
        let names: Vec<&str> = definition
            .columns
            .iter()
            .map(|c| c.column.as_str())
            .collect();
        let find = |name: &str| {
            tables.iter().enumerate().find_map(|(t, (_, table))| {
                table
                    .columns
                    .iter()
                    .find(|c| c.name == name)
                    .map(|c| (t, c))
            })
        };
        let mut sources = Vec::new();
        for name in &names {
            let (t, column) = find(name).ok_or_else(|| NormalizeError::MissingColumn {
                source_id: definition.id.clone(),
                column: (*name).to_owned(),
            })?;
            if !column.readable() {
                return Err(unsupported(format!(
                    "its column {name} is not a single value"
                )));
            }
            sources.push((t, column));
        }
        let identifier = definition
            .columns
            .iter()
            .find(|c| c.field == Field::Identifier)
            .ok_or_else(|| unsupported("its definition maps no identifier column".to_owned()))?;
        // The row of each identifier in each table after the first, which is read in order.
        let mut keys: Vec<HashMap<String, usize>> = Vec::new();
        for (t, (file, table)) in tables.iter().enumerate() {
            let key = table
                .columns
                .iter()
                .find(|c| c.name == identifier.column)
                .ok_or_else(|| {
                    unsupported(format!("{} has no {}", file.display(), identifier.column))
                })?;
            if t > 0 {
                keys.push(
                    (0..table.rows)
                        .map(|r| (table.text(r, key).into_owned(), r))
                        .collect(),
                );
            }
        }
        let plan = Plan::new(definition, &names)?;
        let cx = Context {
            plan: &plan,
            source: &definition.id,
            epoch: definition.epoch,
            correction: self.correction,
        };
        let Some((_, first)) = tables.first() else {
            return Ok(());
        };
        let key = &sources[plan.identifier].1;
        let mut cells = Vec::with_capacity(sources.len());
        for row in 0..first.rows {
            self.go_on(row)?;
            let id = first.text(row, key);
            cells.clear();
            cells.extend(sources.iter().map(|(t, column)| {
                let (_, table) = &tables[*t];
                if *t == 0 {
                    return table.text(row, column);
                }
                keys[*t - 1]
                    .get(id.as_ref())
                    .map_or(Cow::Borrowed(""), |r| table.text(*r, column))
            }));
            self.row(&cx, &cells);
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
    normalize_until(definition, files, correction, &AtomicBool::new(false))
}

/// [`normalize`], stopping with [`NormalizeError::Cancelled`] soon after `cancel` is set.
///
/// # Errors
///
/// Fails as [`normalize`] does, or when `cancel` is set.
pub fn normalize_until(
    definition: &SourceDefinition,
    files: &[PathBuf],
    correction: &CmbCorrection,
    cancel: &AtomicBool,
) -> Result<Normalized, NormalizeError> {
    if let (Some((Holds::StarDistances, _)), [dir]) = (definition.access.repository(), files) {
        return crate::stars::normalize(definition, dir);
    }
    let mut normalizer = Normalizer::new(definition, correction)?.cancelled_by(cancel);
    let read = |file: &PathBuf| {
        std::fs::read(file).map_err(|e| NormalizeError::read(&definition.id, file, &e))
    };
    let tables = match &definition.access {
        Access::Files { files } => file_tables(files),
        _ => None,
    };
    if tables == Some(FileTables::CsvGz) {
        for file in files {
            normalizer.csv_file(file)?;
        }
    } else if tables == Some(FileTables::Fits) {
        let mut tables = Vec::new();
        for file in files {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let table = crate::fits::unpack(&name, read(file)?)
                .and_then(|fits| bintable::first_table(&fits))
                .map_err(|error| NormalizeError::Table {
                    source_id: definition.id.clone(),
                    file: file.clone(),
                    error: error.to_string(),
                })?;
            tables.push((file.clone(), table));
        }
        normalizer.fits(&tables)?;
    } else {
        for file in files {
            normalizer.table(file, &read(file)?)?;
        }
    }
    Ok(normalizer.finish())
}
