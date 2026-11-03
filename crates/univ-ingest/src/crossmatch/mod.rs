//! The cross-match stage: the normalized entries of every source resolved into objects, each
//! ranked for its preferred measurement, given its stable id against the previous release and
//! the id ledger, and placed under the release cosmology; then the member lists of the
//! researched structures resolved to those objects.

mod basis;
mod members;
mod place;
mod tracers;

use std::collections::BTreeMap;

use thiserror::Error;
use univ_model::cosmology::Cosmology;
use univ_model::derive::{DerivedPosition, Release};
use univ_model::ids::ObjectId;
use univ_model::ledger::Ledger;
use univ_model::object::{Object, Placement};
use univ_model::redshift::{CmbCorrection, DipoleModel};

pub use basis::{Basis, RULES};
pub use members::{
    MemberPlacement, MemberProblem, NameIndex, Resolved, UnplacedMember, resolve_all,
};
pub use place::place;
pub use tracers::{FilePlacer, FromFile, STRUCTURE_REDSHIFTS};

use crate::events::{Count, Event, EventSink, Stage};
use crate::ids::{AssignError, Assigned, assign_in_order};
use crate::matching::{MatchReport, match_entries};
use crate::ranking::rank_all;
use crate::state::State;
use crate::watch::Watch;
use univ_model::entry::CatalogueEntry;

/// The steps of the stage, which its status line counts.
pub const STEPS: u64 = 5;

/// What the cross-match works under: the release's rules, configuration and researched
/// structures, its cosmology, and the state of the last release.
pub struct Inputs<'a> {
    pub basis: &'a Basis,
    /// The release cosmology with its D(z) tabulated once for the run.
    pub release: &'a Release,
    /// The run's one CMB dipole correction, the one its entries were normalized with.
    pub correction: &'a CmbCorrection,
    pub state: &'a State,
}

/// An object of the release: its id, the object and its derived position, if placed.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseObject {
    pub id: ObjectId,
    pub object: Object,
    pub position: Option<DerivedPosition>,
}

/// What the cross-match made.
#[derive(Clone, Debug, PartialEq)]
pub struct CrossMatched {
    /// The release the ids were assigned for: the next in the ledger.
    pub release: String,
    /// The release cosmology every derived position was placed under.
    pub cosmology: Cosmology,
    /// The run's CMB dipole, which its entries were normalized and its structures placed with.
    pub dipole: DipoleModel,
    /// The objects, in the order of their first entry.
    pub objects: Vec<ReleaseObject>,
    pub assigned: Assigned,
    /// The ledger with the release recorded, and the entry ids, as the next state; the ids
    /// are kept there alone, not in `assigned`.
    pub next: State,
    pub structures: Vec<MemberPlacement>,
    pub matching: MatchReport,
    /// What the cross-match worked under, which the tiles and the release are built under.
    pub basis: Basis,
}

/// A cross-match that cannot be made.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrossMatchError {
    #[error("the ids cannot be assigned: {0}")]
    Ids(#[from] AssignError),
    #[error("{0}")]
    Stopped(String),
}

/// The name of the release after those `ledger` lists: one more than their number.
#[must_use]
pub fn next_release(ledger: &Ledger) -> String {
    (ledger.releases().len() + 1).to_string()
}

/// Cross-matches `entries` under `inputs`, telling `sink` of each step and of the stage's
/// counts.
///
/// # Errors
///
/// Fails when the ids cannot be assigned, such as a previous release holding an id the ledger
/// never issued.
pub fn cross_match(
    entries: Vec<CatalogueEntry>,
    inputs: &Inputs<'_>,
    sink: &dyn EventSink,
) -> Result<CrossMatched, CrossMatchError> {
    cross_match_watched(entries, inputs, sink, Watch::unwatched())
}

/// [`cross_match`], stopping after the step in hand once the owner asks it to through `watch`.
///
/// # Errors
///
/// Fails as [`cross_match`] does, or when the owner asked it to stop.
pub fn cross_match_watched(
    entries: Vec<CatalogueEntry>,
    inputs: &Inputs<'_>,
    sink: &dyn EventSink,
    watch: Watch<'_>,
) -> Result<CrossMatched, CrossMatchError> {
    let step = |step: String| {
        sink.emit(Event::Stepped {
            stage: Stage::Match,
            step,
        });
        watch.check().map_err(CrossMatchError::Stopped)
    };
    let count = entries.len();
    let basis = inputs.basis;
    let matched = match_entries(&basis.matching, entries);
    let report = matched.report.clone();
    step(format!(
        "matching: {count} entries into {} objects, {} candidate links",
        report.objects, report.candidate_links
    ))?;
    let mut objects = matched.objects;
    rank_all(&mut objects, &basis.ranking, inputs.release);
    let ranked = objects.iter().filter(|o| o.preferred_distance.is_some());
    step(format!(
        "ranking: {} with a preferred distance",
        ranked.count()
    ))?;

    let release = next_release(&inputs.state.ledger);
    let mut ledger = inputs.state.ledger.clone();
    let groups = objects.iter().map(|o| {
        o.entries
            .iter()
            .map(|e| e.entry.designation.clone())
            .collect()
    });
    let (mut assigned, ids) =
        assign_in_order(&inputs.state.previous, &mut ledger, &release, groups)?;
    step(format!(
        "ids: release {release}, {} new, {} replaced",
        issued(&assigned),
        assigned.replaced.len()
    ))?;

    let mut placed = Vec::with_capacity(objects.len());
    for (mut object, id) in objects.into_iter().zip(ids) {
        let position = place(&mut object, inputs.release, &basis.ranking);
        placed.push(ReleaseObject {
            id,
            object,
            position,
        });
    }
    let count = placed.iter().filter(|o| o.position.is_some()).count();
    step(format!("positions: {count} placed of {}", placed.len()))?;

    let resolved: Vec<Resolved<'_>> = placed
        .iter()
        .map(|o| Resolved {
            id: o.id,
            object: &o.object,
            position: o.position.as_ref(),
        })
        .collect();
    let tolerance = basis.matching.structure_members.angular_tolerance_arcsec;
    let tracers = FilePlacer::new(inputs.release, inputs.correction);
    let structures = resolve_all(
        &basis.structures(),
        &NameIndex::new(&resolved),
        tolerance,
        Some(&tracers),
    );
    for s in structures.iter().filter(|s| s.shape.is_none()) {
        let (no_object, unplaced) = s.unresolved_and_unplaced();
        sink.emit(Event::MembersUnplaced {
            structure: s.structure.clone(),
            members: s.members as u64,
            no_object,
            unplaced,
            point: s.point.is_some(),
        });
    }
    let from_file: usize = structures.iter().map(|s| s.from_file.len()).sum();
    step(format!(
        "members: {} member lists, {from_file} members placed from their own cited position and \
         redshift",
        structures.len()
    ))?;

    let next = State {
        ledger,
        previous: std::mem::take(&mut assigned.entries),
    };
    let crossed = CrossMatched {
        release,
        cosmology: inputs.release.cosmology().clone(),
        dipole: inputs.correction.model(),
        objects: placed,
        assigned,
        next,
        structures,
        matching: report,
        basis: basis.clone(),
    };
    sink.emit(Event::StageEnded {
        stage: Stage::Match,
        counts: crossed.counts(),
    });
    Ok(crossed)
}

impl CrossMatched {
    /// What the stage counts: objects, merges, candidate links, ids, placed and not placed by
    /// reason, and the member lists placed and shown as points.
    #[must_use]
    pub fn counts(&self) -> Vec<Count> {
        let m = &self.matching;
        let mut counts = vec![
            Count::new(m.entries, "entries"),
            Count::new(m.objects, "objects"),
            Count::new(m.cross_identified + m.merged_by_position, "merges"),
            Count::new(m.candidate_links, "candidate links"),
            Count::new(issued(&self.assigned), "new ids"),
            Count::new(self.assigned.replaced.len() as u64, "ids replaced"),
        ];
        let mut not_placed: BTreeMap<&str, u64> = BTreeMap::new();
        let mut placed = 0;
        for o in &self.objects {
            match o.object.placement {
                Placement::Placed if o.position.is_some() => placed += 1,
                Placement::Placed => {}
                Placement::NotPlaced { reason } => {
                    *not_placed.entry(reason.describe()).or_default() += 1;
                }
            }
        }
        counts.push(Count::new(placed, "placed"));
        for (reason, n) in not_placed {
            counts.push(Count::new(n, format!("not placed ({reason})")));
        }
        let lists = self.structures.len() as u64;
        let whole = self.structures.iter().filter(|s| s.shape.is_some()).count() as u64;
        counts.push(Count::new(whole, "member lists placed"));
        counts.push(Count::new(lists - whole, "member lists not placed"));
        let unchecked: usize = self.structures.iter().map(|s| s.by_name_only.len()).sum();
        counts.push(Count::new(
            unchecked as u64,
            "members resolved by name only",
        ));
        counts
    }
}

/// The number of ids the release issued.
fn issued(assigned: &Assigned) -> u64 {
    assigned
        .issued
        .map_or(0, |r| r.last.get() - r.first.get() + 1)
}
