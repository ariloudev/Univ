//! Cross-matching: catalogue entries of several sources resolved into objects. Published
//! cross-identifications are applied first, then matches by position and by redshift or
//! distance under the release's matching rules (`rules/matching.yaml`). An entry with more than
//! one candidate from one source is kept apart, linked to each as a candidate match.

mod assemble;
mod check;
mod cross_id;
pub mod extent;
mod fibres;
mod fields;
mod groups;
mod index;
mod pieces;
mod positions;
mod relations;
pub mod rules;
mod sight;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::{Designation, SourceId};
use univ_model::object::Object;

pub use index::SkyIndex;
pub use rules::MatchingRules;

use groups::Groups;
use rules::{Comparison, PairRule};
use univ_model::position::Vec3;

/// The objects resolved from a set of entries.
#[derive(Clone, Debug, PartialEq)]
pub struct Matched {
    /// The objects, each boxed where it is made: a release object holds its object boxed, so
    /// it is never moved again, and the objects are never held twice.
    pub objects: Vec<Box<Object>>,
    pub report: MatchReport,
}

impl Matched {
    /// The object holding the entry `designation`, by a scan of the objects.
    #[must_use]
    pub fn find(&self, designation: &Designation) -> Option<&Object> {
        self.position_of(designation).map(|i| &*self.objects[i])
    }

    /// The place in `objects` of the object holding `designation`, by a scan of the objects.
    #[must_use]
    pub fn position_of(&self, designation: &Designation) -> Option<usize> {
        self.objects
            .iter()
            .position(|o| o.entry(designation).is_some())
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
    /// Relations between a galaxy group and its dominant galaxy or a member galaxy.
    #[serde(default)]
    pub group_relations: u64,
    /// Groups joined to a nearby galaxy as pieces within its published extent; each is also
    /// counted among the merges by position.
    #[serde(default)]
    pub pieces_within_extent: u64,
    /// Joins of two groups as fibres of one survey on one nearby galaxy without an extent; each
    /// is also counted among the merges by position.
    #[serde(default)]
    pub fibres_of_one_galaxy: u64,
    /// Objects whose entries' sources name different kinds of object, such as a Gaia star that
    /// one source lists as a Cepheid and another as an RR Lyrae star: each takes the kind of
    /// its first such entry.
    #[serde(default)]
    pub conflicting_kinds: u64,
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
    /// The pair rule of two source numbers `a` and `b`, at `a * sources.len() + b`.
    pairs: Vec<Option<&'a PairRule>>,
    /// Each entry's ICRS unit vector, where it has a position.
    units: Vec<Option<Vec3>>,
    groups: Groups,
    joins: Vec<Edge>,
    /// Candidate links, each with its confidence.
    links: Vec<(Edge, f64)>,
    /// For each group that took pieces within an extent, by root: the galaxy that took them,
    /// or, where that galaxy was itself taken in as a piece of a larger group, that group's.
    hosts: HashMap<usize, usize>,
}

/// Resolves `entries` into objects under `rules`: cross-identifications, then positions, then
/// the pieces of nearby galaxies within their published extents, then the fibres of one survey
/// on one nearby galaxy without one, then the relations of galaxy groups to their dominant
/// galaxies.
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
    let units = entries
        .iter()
        .map(|e| e.position.as_ref().map(|p| p.icrs.unit_vector()))
        .collect();
    let mut work = Work {
        rules,
        entries: &entries,
        groups: Groups::new(&source_of),
        source_of,
        pairs: rules.pair_table(&sources),
        sources,
        units,
        joins: Vec::new(),
        links: Vec::new(),
        hosts: HashMap::new(),
    };
    cross_id::apply(&mut work);
    positions::apply(&mut work);
    // The entries of every group, which the extent and fibre passes share and keep up to date:
    // built only when the rules have an extent rule, which both passes need.
    let (pieces, fibres) = if rules.extents.is_some() {
        let mut members = pieces::Members::new(&mut work);
        let pieces = pieces::apply(&mut work, &mut members);
        (pieces, fibres::apply(&mut work, &mut members))
    } else {
        (0, 0)
    };
    let plan = assemble::plan(&mut work);
    let relations = relations::find(&work);
    let mut matched = assemble::build(rules, entries, plan, relations);
    matched.report.pieces_within_extent = pieces;
    matched.report.fibres_of_one_galaxy = fibres;
    matched
}

/// A join of `a` and `b` by position under the comparison `c`.
fn edge(a: usize, b: usize, c: Comparison) -> Edge {
    Edge {
        a,
        b,
        join: Join::Position(c),
    }
}

impl Work<'_> {
    /// For each source of the run, by number, whether `list` names it.
    fn in_list(&self, list: &[SourceId]) -> Vec<bool> {
        self.sources.iter().map(|s| list.contains(s)).collect()
    }

    /// The pair rule of the sources of entries `a` and `b`.
    fn pair(&self, a: usize, b: usize) -> Option<&PairRule> {
        let n = self.sources.len();
        self.pairs[usize::from(self.source_of[a]) * n + usize::from(self.source_of[b])]
    }

    /// Compares entries `a` and `b` under their pair rule: none when no rule pairs their
    /// sources, either lacks a position, or they lie beyond a tolerance.
    fn compare(&self, a: usize, b: usize) -> Option<Comparison> {
        let pair = self.pair(a, b)?;
        let (ua, ub) = (self.units[a]?, self.units[b]?);
        let separation = univ_model::position::separation_degrees(ua, ub) * 3_600.0;
        self.rules
            .compare_at(pair, &self.entries[a], &self.entries[b], separation)
    }
}
