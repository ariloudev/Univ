//! Preferred measurements: the distance that places each object, by the smallest fractional
//! uncertainty with the method rank breaking ties, and the sky position it takes, under the
//! release's ranking rule (`rules/ranking.yaml`).

mod distance;
pub mod rules;

use univ_model::derive::Release;
use univ_model::entry::PositionUncertainty;
use univ_model::object::{NotPlacedReason, Object, Placement, PreferredPosition};

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
pub fn rank_all(objects: &mut [Object], rules: &RankingRules, release: &Release) {
    for object in objects {
        rank(object, rules, release);
    }
}

/// The position of the entry with the smallest published error; entries without one after,
/// and ties by class, then by the order of the entries.
#[must_use]
pub fn preferred_position(object: &Object, rules: &RankingRules) -> Option<PreferredPosition> {
    object
        .entries
        .iter()
        .enumerate()
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
