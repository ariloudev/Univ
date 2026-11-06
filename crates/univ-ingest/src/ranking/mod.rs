//! Preferred measurements: the distance that places each object, its redshift distances ordered
//! among themselves by kind of value and velocity uncertainty and the leading one weighed against
//! the other distances by the smallest fractional uncertainty with the method rank breaking
//! ties, and the sky position it takes, under the release's ranking rule (`rules/ranking.yaml`).

mod distance;
mod redshift;
pub mod rules;

use std::collections::HashMap;

use univ_model::derive::Release;
use univ_model::entry::PositionUncertainty;
use univ_model::ids::Designation;
use univ_model::object::{
    MatchMethod, NotPlacedReason, Object, ObjectEntry, Placement, PreferredPosition, Resolution,
};

pub use distance::{Candidate, Which, candidates, choose, fraction, redshift_distance};
pub use rules::RankingRules;

/// Ranks `object` under the release cosmology of `release`, whose table of D(z) is built once
/// per run: its preferred distance and position, and whether it can be placed. An object whose
/// preferred distance is more uncertain than the rule's `too_uncertain_above` (100%) keeps it,
/// for look-ups, but is not placed.
pub fn rank(object: &mut Object, rules: &RankingRules, release: &Release) {
    let found = candidates(object, rules, release);
    object.preferred_distance = choose(object, &found, rules);
    object.preferred_position = preferred_position(object, rules);
    let reason = match &object.preferred_distance {
        None => Some(NotPlacedReason::NoDistance),
        Some(p)
            if p.uncertainty
                .total()
                .is_some_and(|f| f > rules.too_uncertain_above) =>
        {
            Some(NotPlacedReason::DistanceTooUncertain)
        }
        Some(_) => None,
    };
    object.placement = reason.map_or(Placement::Placed, |reason| Placement::NotPlaced { reason });
}

/// Ranks every object.
pub fn rank_all(objects: &mut [Box<Object>], rules: &RankingRules, release: &Release) {
    for object in objects {
        rank(object, rules, release);
    }
}

/// How `entry` joined its object, when it joined by a match.
fn method(entry: &ObjectEntry) -> Option<MatchMethod> {
    match &*entry.resolved_by {
        Resolution::Match(r) => Some(r.method),
        _ => None,
    }
}

/// Which entries of `object` are pieces of a nearby galaxy that took them within its extent:
/// those whose resolution leads back to the galaxy through a join of the extent rule or of the
/// fibre rule. Empty for an object that took no piece within an extent, as nearly all are. The
/// entries are in the order matching reached them, each after the entry it joined through.
fn pieces(object: &Object) -> Vec<bool> {
    if !object
        .entries
        .iter()
        .any(|e| method(e) == Some(MatchMethod::WithinExtent))
    {
        return Vec::new();
    }
    let index: HashMap<&Designation, usize> = object
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (&e.entry.designation, i))
        .collect();
    let mut piece = vec![false; object.entries.len()];
    for (i, e) in object.entries.iter().enumerate() {
        let through = match &*e.resolved_by {
            Resolution::Match(r) => Some(&r.with),
            Resolution::CrossIdentification { via, .. } => Some(via),
            Resolution::Anchor => None,
        };
        // The entry it joined through comes before it.
        let parent = through
            .and_then(|d| index.get(d).copied())
            .filter(|&p| p < i);
        piece[i] = matches!(
            method(e),
            Some(MatchMethod::WithinExtent | MatchMethod::FibresOfOneGalaxy)
        ) || parent.is_some_and(|p| piece[p]);
    }
    piece
}

/// The position of the entry with the smallest published error; entries without one after,
/// and ties by class, then by the order of the entries. An object that took pieces of a nearby
/// galaxy takes the position of the galaxy whose extent took them, never a piece's: a piece lies
/// off the galaxy's centre by construction.
#[must_use]
pub fn preferred_position(object: &Object, rules: &RankingRules) -> Option<PreferredPosition> {
    let piece = pieces(object);
    object
        .entries
        .iter()
        .enumerate()
        .filter(|(i, _)| !piece.get(*i).copied().unwrap_or(false))
        .filter_map(|(i, e)| {
            let p = e.entry.position.as_ref()?;
            let error = match &p.uncertainty {
                PositionUncertainty::Published(errors) => {
                    let (a, d) = (errors.ra.normalized.value, errors.dec.normalized.value);
                    Some((a * a).midpoint(d * d).sqrt() * 3_600_000.0)
                }
                PositionUncertainty::NoneReported => None,
            };
            let class = rules.class_of(&e.entry.designation.source);
            Some((i, error, class))
        })
        .min_by(|(i, ea, ca), (j, eb, cb)| {
            let key = |e: &Option<f64>| (e.is_none(), e.unwrap_or(0.0));
            let (ka, kb) = (key(ea), key(eb));
            ka.0.cmp(&kb.0)
                .then(ka.1.total_cmp(&kb.1))
                .then(ca.cmp(cb))
                .then(i.cmp(j))
        })
        .map(|(i, error, class)| PreferredPosition {
            entry: object.entries[i].entry.designation.clone(),
            class,
            uncertainty_mas: error,
        })
}
