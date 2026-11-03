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
/// CON-3.
pub const D1_SIZE: &str = "the release's D1 rows fit within 10 GB";
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
