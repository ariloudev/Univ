//! Cross-matching: catalogue entries of several sources resolved into objects. Published
//! cross-identifications are applied first, then matches by position and by redshift or
//! distance under the release's matching rules (`rules/matching.yaml`). An entry with more than
//! one candidate from one source is kept apart, linked to each as a candidate match.

mod assemble;
mod cross_id;
mod groups;
mod index;
mod positions;
pub mod rules;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::{Designation, SourceId};
use univ_model::object::Object;

pub use index::SkyIndex;
pub use rules::MatchingRules;

use groups::Groups;
use rules::Comparison;

/// The objects resolved from a set of entries, with a lookup by designation.
#[derive(Clone, Debug, PartialEq)]
pub struct Matched {
    pub objects: Vec<Object>,
    pub report: MatchReport,
    lookup: HashMap<Designation, usize>,
}

impl Matched {
    /// The object holding the entry `designation`.
    #[must_use]
    pub fn find(&self, designation: &Designation) -> Option<&Object> {
        self.lookup.get(designation).map(|i| &self.objects[*i])
    }

    /// The place in `objects` of the object holding `designation`.
    #[must_use]
    pub fn position_of(&self, designation: &Designation) -> Option<usize> {
        self.lookup.get(designation).copied()
    }
}

/// What matching did, as the release's provenance records it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchReport {
    pub entries: u64,
    pub objects: u64,
    /// Entries joined to an object by a published cross-identification.
    pub cross_identified: u64,
    /// Entries joined to an object by position.
    pub merged_by_position: u64,
    /// Candidate links between objects.
    pub candidate_links: u64,
    /// Objects of more than one entry.
    pub multi_entry_objects: u64,
}

/// Why two entries were joined.
#[derive(Clone, Debug)]
enum Join {
    Cross { scheme: usize, identifier: String },
    Position(Comparison),
}

/// A join, or a candidate link, between the entries `a` and `b`.
#[derive(Clone, Debug)]
struct Edge {
    a: usize,
    b: usize,
    join: Join,
}

/// The state of one matching run.
struct Work<'a> {
    rules: &'a MatchingRules,
    entries: &'a [CatalogueEntry],
    /// Each entry's source number, and the source of each number.
    source_of: Vec<u16>,
    sources: Vec<SourceId>,
    groups: Groups,
    joins: Vec<Edge>,
    /// Candidate links, each with its confidence.
    links: Vec<(Edge, f64)>,
}

/// Resolves `entries` into objects under `rules`: cross-identifications, then positions.
/// Objects are in the order of their first entry, so a run on the same entries gives the same
/// objects.
#[must_use]
pub fn match_entries(rules: &MatchingRules, entries: Vec<CatalogueEntry>) -> Matched {
    let mut sources: Vec<SourceId> = Vec::new();
    let mut numbers: HashMap<SourceId, u16> = HashMap::new();
    let source_of: Vec<u16> = entries
        .iter()
        .map(|e| {
            let s = &e.designation.source;
            *numbers.entry(s.clone()).or_insert_with(|| {
                sources.push(s.clone());
                u16::try_from(sources.len() - 1).unwrap_or(u16::MAX)
            })
        })
        .collect();
    let mut work = Work {
        rules,
        entries: &entries,
        groups: Groups::new(&source_of),
        source_of,
        sources,
        joins: Vec::new(),
        links: Vec::new(),
    };
    cross_id::apply(&mut work);
    positions::apply(&mut work);
    let plan = assemble::plan(&mut work);
    assemble::build(rules, entries, plan)
}
