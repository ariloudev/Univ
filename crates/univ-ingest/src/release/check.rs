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
use univ_model::release::D1_CAP_BYTES;
use univ_model::release::manifest::{D1Estimate, TableEstimate};
use univ_model::release::shards::Database;
use univ_model::tile::decode;

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
/// CON-3, the cap being `D1_CAP_BYTES`: one release within `RELEASE_BUDGET_BYTES`, so the next
/// is published beside it before it is pruned.
pub const D1_SIZE: &str = "the release's D1 rows fit within the release budget";
/// Every derived position is held by its object's row, its preferred measurement, its chain
/// and the release.
pub const D1_DERIVED: &str = "every derived position is held by the D1 rows";
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

/// Each table with rows, its rows and estimated bytes, largest first: `objects 2095269 rows
/// 1.2 GB`.
#[must_use]
pub fn table_sizes(estimate: &D1Estimate) -> Vec<String> {
    let mut tables: Vec<&TableEstimate> = estimate.tables.iter().filter(|t| t.rows > 0).collect();
    tables.sort_by_key(|t| std::cmp::Reverse(t.estimated_bytes));
    tables
        .iter()
        .map(|t| {
            format!(
                "{} {} rows {}",
                t.table,
                t.rows,
                decimal_bytes(t.estimated_bytes)
            )
        })
        .collect()
}

/// CON-3: the estimate of the release's rows against the release budget, half the cap less a
/// margin, so release n + 1 can be published before release n is pruned.
#[must_use]
pub fn d1_size(database: Database, estimate: &D1Estimate, budget: u64) -> Option<Failed> {
    let estimated = estimate.estimated_bytes;
    (estimated > budget).then(|| Failed {
        check: D1_SIZE,
        cause: format!(
            "its rows in {database} are estimated at {estimated} bytes, beyond the {} budget of a \
             release in one database ({budget} bytes, so two fit the {} D1 cap); by table: {}",
            decimal_bytes(budget),
            decimal_bytes(D1_CAP_BYTES),
            table_sizes(estimate).join(", ")
        ),
    })
}

/// The objects whose derived position the rows do not give back, the first named.
#[must_use]
pub fn d1_derived(not_kept: u64, first: Option<&str>) -> Option<Failed> {
    first.map(|first| Failed {
        check: D1_DERIVED,
        cause: format!("{not_kept} placed object(s) would lose part of it, from {first}"),
    })
}
