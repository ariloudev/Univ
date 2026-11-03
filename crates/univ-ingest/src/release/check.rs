//! The checks a release passes before it is complete (RQ-34): every object a tile holds is
//! among its objects (AC-86), every number of every structure cites its source (AC-87), the
//! entries of each source match its provenance (AC-88), and no source forbids redistribution
//! (AC-89). Each failure names its check and its cause, and every check runs, so one build
//! names every failure.
//!
//! Sources whose redistribution review is still provisional are reported, not refused: a local
//! build may hold them, and refusing them is the gate of a public deployment.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use univ_model::ids::{ObjectId, SourceId};
use univ_model::release::{D1_CAP_BYTES, SqlValue, TABLES, Table};
use univ_model::tile::decode;

use super::D1Rows;
use crate::crossmatch::ReleaseObject;
use crate::normalize::reads_rows;
use crate::provenance::Provenance;
use crate::source::{Review, SourceDefinition};
use crate::structures::Structure;
use crate::tiles::{Built, name_of};

/// AC-86.
pub const TILE_OBJECTS: &str = "every object a tile holds is among the release's objects";
/// AC-87.
pub const STRUCTURE_CITATIONS: &str = "every number of every structure cites its source";
/// AC-88.
pub const ENTRY_COUNTS: &str = "each source's entries match its provenance";
/// AC-89.
pub const REDISTRIBUTION: &str = "no source forbids redistribution";
/// CON-3, the cap being `D1_CAP_BYTES`.
pub const D1_SIZE: &str = "the release's D1 rows fit within the D1 cap";
/// No NULL is written for a number that is not finite.
pub const D1_FINITE: &str = "every number of every D1 row is finite";
/// `INSERT OR IGNORE` skips only a row written before, never another row with the same key.
pub const D1_KEYS: &str = "each D1 table's primary keys are unique";
/// AC-84, once written.
pub const MANIFEST: &str = "every file matches its SHA-256 in the release manifest";

/// A check that failed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub check: &'static str,
    pub cause: String,
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the check that {} failed: {}", self.check, self.cause)
    }
}

/// What the checks look at.
pub struct Subject<'a> {
    /// The release's D1 rows.
    pub d1: &'a D1Rows,
    pub objects: &'a [ReleaseObject],
    pub tiles: &'a Built,
    pub definitions: &'a [SourceDefinition],
    pub structures: &'a [(String, Structure)],
    /// The catalogue entries the release holds from each source.
    pub entries: &'a BTreeMap<SourceId, u64>,
    /// The cache, which keeps each source's provenance.
    pub cache: &'a Path,
}

/// Every check of `subject`, giving each failure.
#[must_use]
pub fn check_all(subject: &Subject<'_>) -> Vec<Failed> {
    let mut failed = Vec::new();
    tile_objects(subject, &mut failed);
    structure_citations(subject, &mut failed);
    entry_counts(subject, &mut failed);
    d1_size(subject.d1, &mut failed);
    d1_rows(subject.d1, &mut failed);
    for d in subject.definitions {
        if let Err(forbidden) = d.publishable() {
            failed.push(Failed {
                check: REDISTRIBUTION,
                cause: forbidden.to_string(),
            });
        }
    }
    failed
}

/// The sources among `definitions` whose redistribution review is provisional.
#[must_use]
pub fn provisional(definitions: &[SourceDefinition]) -> Vec<SourceId> {
    definitions
        .iter()
        .filter(|d| d.redistribution.review == Review::Provisional)
        .map(|d| d.id.clone())
        .collect()
}

/// AC-86: each tile decoded, each of its object ids looked up.
fn tile_objects(subject: &Subject<'_>, failed: &mut Vec<Failed>) {
    let ids: BTreeSet<ObjectId> = subject.objects.iter().map(|o| o.id).collect();
    for domain in &subject.tiles.domains {
        for node in &domain.nodes {
            let at = format!("{} tile {}", name_of(domain.domain), node.address);
            match decode(&node.tile) {
                Err(e) => failed.push(Failed {
                    check: TILE_OBJECTS,
                    cause: format!("the {at} cannot be decoded: {e}"),
                }),
                Ok(tile) => {
                    let missing: BTreeSet<ObjectId> = tile
                        .points
                        .iter()
                        .map(|p| p.id)
                        .filter(|id| !ids.contains(id))
                        .collect();
                    if let Some(first) = missing.first() {
                        failed.push(Failed {
                            check: TILE_OBJECTS,
                            cause: format!(
                                "the {at} holds {} object id(s) the release has no object for, \
                                 from {first}",
                                missing.len()
                            ),
                        });
                    }
                }
            }
        }
    }
}

/// AC-87: each structure checked again as its loader checks it.
fn structure_citations(subject: &Subject<'_>, failed: &mut Vec<Failed>) {
    for (id, structure) in subject.structures {
        if let Err((field, problem)) = structure.check_cited() {
            failed.push(Failed {
                check: STRUCTURE_CITATIONS,
                cause: format!("structure {id}: {field}: {problem}"),
            });
        }
    }
}

/// AC-88: the entries of each source of rows against what its provenance recorded as published.
fn entry_counts(subject: &Subject<'_>, failed: &mut Vec<Failed>) {
    let mut fail = |cause: String| {
        failed.push(Failed {
            check: ENTRY_COUNTS,
            cause,
        });
    };
    let known: BTreeSet<&SourceId> = subject.definitions.iter().map(|d| &d.id).collect();
    for source in subject.entries.keys().filter(|s| !known.contains(s)) {
        fail(format!(
            "the release holds entries of {source}, which it has no definition for"
        ));
    }
    for d in subject.definitions.iter().filter(|d| reads_rows(d)) {
        let held = subject.entries.get(&d.id).copied().unwrap_or(0);
        let dir = subject.cache.join(d.id.as_str());
        match Provenance::read(&dir).map(|p| p.normalization) {
            Err(e) => fail(format!("{}: its provenance cannot be read: {e}", d.id)),
            Ok(None) => fail(format!(
                "{}: its provenance records no normalization of its payloads",
                d.id
            )),
            Ok(Some(report)) if report.published != held => fail(format!(
                "{}: the release holds {held} entries, but its provenance records {} published",
                d.id, report.published
            )),
            Ok(Some(_)) => {}
        }
    }
}

/// A size in decimal units, as the D1 cap is given, without trailing zeros: `10 GB` for
/// [`D1_CAP_BYTES`], `1.25 GB`, `3.5 MB`.
#[must_use]
pub fn decimal_bytes(bytes: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "a size shown to three figures")]
    let b = bytes as f64;
    let (value, unit) = match bytes {
        0..1_000 => return format!("{bytes} B"),
        1_000..1_000_000 => (format!("{:.1}", b / 1e3), "kB"),
        1_000_000..1_000_000_000 => (format!("{:.1}", b / 1e6), "MB"),
        _ => (format!("{:.2}", b / 1e9), "GB"),
    };
    format!(
        "{} {unit}",
        value.trim_end_matches('0').trim_end_matches('.')
    )
}

/// CON-3: the estimate of the release's rows against the cap.
fn d1_size(d1: &D1Rows, failed: &mut Vec<Failed>) {
    let estimated = d1.estimate().estimated_bytes;
    if estimated > D1_CAP_BYTES {
        failed.push(Failed {
            check: D1_SIZE,
            cause: format!(
                "its rows are estimated at {estimated} bytes, beyond the {} cap of {D1_CAP_BYTES} \
                 bytes",
                decimal_bytes(D1_CAP_BYTES)
            ),
        });
    }
}

/// A row's primary key, as its columns and SQL literals.
fn key_of(table: &Table, row: &[SqlValue]) -> String {
    let parts: Vec<String> = table
        .key
        .iter()
        .filter_map(|c| {
            let v = table.position(c).and_then(|i| row.get(i))?;
            Some(format!("{c} = {}", v.literal()))
        })
        .collect();
    format!("({})", parts.join(", "))
}

/// Every number finite, naming the table, row key and column of the first that is not in each
/// table; and every primary key given once in its table.
fn d1_rows(d1: &D1Rows, failed: &mut Vec<Failed>) {
    for table in &TABLES {
        let rows = d1.table(table.name);
        let not_finite = rows.iter().find_map(|row| {
            row.iter()
                .zip(table.columns)
                .find_map(|(v, column)| match v {
                    SqlValue::Real(x) if !x.is_finite() => Some((row, column, *x)),
                    _ => None,
                })
        });
        if let Some((row, column, x)) = not_finite {
            failed.push(Failed {
                check: D1_FINITE,
                cause: format!(
                    "table {}, row {}, column {column}: {x} is not a finite number",
                    table.name,
                    key_of(table, row)
                ),
            });
        }
        let mut seen = BTreeSet::new();
        let repeated: Vec<String> = rows
            .iter()
            .map(|row| key_of(table, row))
            .filter(|key| !seen.insert(key.clone()))
            .collect();
        if let Some(first) = repeated.first() {
            failed.push(Failed {
                check: D1_KEYS,
                cause: format!(
                    "table {}: {} row(s) repeat a primary key another row has, from {first}",
                    table.name,
                    repeated.len()
                ),
            });
        }
    }
}
