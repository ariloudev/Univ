//! The fourth pass: fibres of one fibre survey on one nearby galaxy that no source of the release
//! gives an extent, such as a survey's several targets on the HII regions of one dwarf. Below the
//! extent rule's cz, two groups join when each fibre of either lies within the fibre rule's angle
//! and the extent rule's velocity tolerance of every fibre of the other (complete linkage), unless
//! both hold an entry of an identity source, they share an entry's source other than a fibre
//! survey, either holds a galaxy that publishes its extent (the extent pass's to take pieces), or
//! a candidate link joins them.

use std::collections::HashMap;

use univ_model::object::{LineOfSight, MatchMethod};
use univ_model::position::separation_degrees;

use super::index::SkyIndex;
use super::pieces::Members;
use super::rules::Comparison;
use super::{Work, edge};

/// What the pass did: the number of joins of two groups as fibres of one galaxy. `members`
/// holds the groups' entries as the extent pass left them, kept up to date as this pass joins
/// them.
pub(super) fn apply(work: &mut Work<'_>, members: &mut Members) -> u64 {
    let Some(rule) = work.rules.extents.as_ref() else {
        return 0;
    };
    let Some(fibres) = rule.fibres.as_ref() else {
        return 0;
    };
    let (radius, tolerance) = (fibres.radius_arcsec / 3_600.0, rule.tolerance_km_s);
    let (survey, identity) = (
        work.in_list(&fibres.surveys),
        work.in_list(&rule.identities),
    );
    // The nearby fibres, each entry of a fibre survey with a position and a cz below the rule's,
    // with their cz.
    let mut nearby: HashMap<usize, f64> = HashMap::new();
    let mut at = Vec::new();
    let mut positions = Vec::new();
    for (e, entry) in work.entries.iter().enumerate() {
        if !survey[usize::from(work.source_of[e])] || work.units[e].is_none() {
            continue;
        }
        let (Some(p), Some(v)) = (&entry.position, work.rules.velocity(entry)) else {
            continue;
        };
        if v < rule.below_km_s {
            nearby.insert(e, v);
            at.push(e);
            positions.push((p.icrs.ra, p.icrs.dec));
        }
    }
    if at.is_empty() {
        return 0;
    }
    let index = SkyIndex::new(&positions, radius);
    let mut near = Vec::new();
    let mut joined = 0;
    for (k, &a) in at.iter().enumerate() {
        let (ra, dec) = positions[k];
        index.near(ra, dec, &mut near);
        for &j in &near {
            let b = at[j as usize];
            if b <= a || work.source_of[a] != work.source_of[b] {
                continue;
            }
            let (root_a, root_b) = (work.groups.find(a), work.groups.find(b));
            if root_a == root_b || work.groups.linked(a, b) {
                continue;
            }
            let (group_a, group_b) = (members.of(root_a), members.of(root_b));
            let source = |e: usize| usize::from(work.source_of[e]);
            let holds_identity = |g: &[usize]| g.iter().any(|&e| identity[source(e)]);
            let has_extent = |g: &[usize]| {
                g.iter()
                    .any(|&e| rule.radius_degrees(&work.entries[e]).is_some())
            };
            let shares_other_source = group_a
                .iter()
                .any(|&x| !survey[source(x)] && group_b.iter().any(|&y| source(y) == source(x)));
            if (holds_identity(&group_a) && holds_identity(&group_b))
                || has_extent(&group_a)
                || has_extent(&group_b)
                || shares_other_source
            {
                continue;
            }
            let Some((separation, delta)) =
                linkage(work, &nearby, (&group_a, &group_b), radius, tolerance)
            else {
                continue;
            };
            let comparison = Comparison {
                method: MatchMethod::FibresOfOneGalaxy,
                separation_arcsec: separation * 3_600.0,
                difference: Some(LineOfSight::Velocity { delta_km_s: delta }),
                confidence: work.rules.positional_confidence(
                    separation,
                    radius,
                    work.rules.line_of_sight_factor(delta, tolerance),
                ),
            };
            work.groups.absorb(a, b);
            let mut all = group_a;
            all.extend(group_b);
            members.merge(&[root_a, root_b], work.groups.find(a), all);
            work.joins.push(edge(a, b, comparison));
            joined += 1;
        }
    }
    joined
}

/// Whether every nearby fibre of one group lies within `radius` degrees and `tolerance` km/s of
/// every nearby fibre of the other: the widest separation and velocity difference across them,
/// or none. Each group holds at least the fibre it was found by.
fn linkage(
    work: &Work<'_>,
    nearby: &HashMap<usize, f64>,
    (a, b): (&[usize], &[usize]),
    radius: f64,
    tolerance: f64,
) -> Option<(f64, f64)> {
    let fibres = |g: &[usize]| -> Vec<(usize, f64)> {
        g.iter()
            .filter_map(|e| nearby.get(e).map(|&v| (*e, v)))
            .collect()
    };
    let (fa, fb) = (fibres(a), fibres(b));
    let mut widest = (0.0_f64, 0.0_f64);
    for &(x, vx) in &fa {
        for &(y, vy) in &fb {
            let s = separation_degrees(work.units[x]?, work.units[y]?);
            let d = (vx - vy).abs();
            if s > radius || d > tolerance {
                return None;
            }
            widest = (widest.0.max(s), widest.1.max(d));
        }
    }
    Some(widest)
}
