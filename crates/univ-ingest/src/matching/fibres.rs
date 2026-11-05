//! The fourth pass: fibres of one fibre survey on one nearby galaxy that no source of the release
//! gives an extent, such as a survey's several targets on the HII regions of one dwarf. Below the
//! extent rule's cz, two groups join when each fibre of either lies within the fibre rule's angle
//! and the extent rule's velocity tolerance of every fibre of the other (complete linkage), unless
//! both hold an entry of an identity source, or a candidate link joins them.

use univ_model::object::{LineOfSight, MatchMethod};
use univ_model::position::separation_degrees;

use super::index::SkyIndex;
use super::pieces::Members;
use super::rules::Comparison;
use super::{Edge, Join, Work};

/// What the pass did: the number of joins of two groups as fibres of one galaxy.
pub(super) fn apply(work: &mut Work<'_>) -> u64 {
    let Some(rule) = work.rules.extents.as_ref() else {
        return 0;
    };
    let Some(fibres) = rule.fibres.as_ref() else {
        return 0;
    };
    let radius = fibres.radius_arcsec / 3_600.0;
    let in_list = |list: &[univ_model::ids::SourceId]| -> Vec<bool> {
        work.sources.iter().map(|s| list.contains(s)).collect()
    };
    let (survey, identity) = (in_list(&fibres.surveys), in_list(&rule.identities));
    // The nearby fibres: each entry of a fibre survey with a position and a cz below the rule's.
    let mut nearby: Vec<Option<f64>> = vec![None; work.entries.len()];
    let mut at = Vec::new();
    let mut positions = Vec::new();
    for (e, entry) in work.entries.iter().enumerate() {
        if !survey[usize::from(work.source_of[e])] {
            continue;
        }
        let (Some(p), Some(v)) = (&entry.position, work.rules.velocity(entry)) else {
            continue;
        };
        if v < rule.below_km_s && work.units[e].is_some() {
            nearby[e] = Some(v);
            at.push(e);
            positions.push((p.icrs.ra, p.icrs.dec));
        }
    }
    if at.is_empty() {
        return 0;
    }
    let index = SkyIndex::new(&positions, radius);
    let mut members = Members::new(work);
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
            let holds_identity =
                |g: &[usize]| g.iter().any(|&e| identity[usize::from(work.source_of[e])]);
            if holds_identity(&group_a) && holds_identity(&group_b) {
                continue;
            }
            let Some((separation, delta)) = linkage(work, &nearby, &group_a, &group_b, radius)
            else {
                continue;
            };
            let tolerance = rule.tolerance_km_s;
            let factor = 1.0 - (delta / tolerance).powf(work.rules.confidence.exponent);
            let comparison = Comparison {
                method: MatchMethod::FibresOfOneGalaxy,
                separation_arcsec: separation * 3_600.0,
                difference: Some(LineOfSight::Velocity { delta_km_s: delta }),
                confidence: work.rules.positional_confidence(separation, radius, factor),
            };
            work.groups.absorb(a, b);
            members.joined.remove(&root_a);
            members.joined.remove(&root_b);
            let mut all = group_a;
            all.extend(group_b);
            members.joined.insert(work.groups.find(a), all);
            work.joins.push(Edge {
                a,
                b,
                join: Join::Position(comparison),
            });
            joined += 1;
        }
    }
    joined
}

/// Whether every nearby fibre of one group lies within `radius` degrees and the extent rule's
/// velocity tolerance of every nearby fibre of the other: the widest separation and velocity
/// difference across them, or none.
fn linkage(
    work: &Work<'_>,
    nearby: &[Option<f64>],
    a: &[usize],
    b: &[usize],
    radius: f64,
) -> Option<(f64, f64)> {
    let tolerance = work.rules.extents.as_ref()?.tolerance_km_s;
    let fibres = |g: &[usize]| -> Vec<(usize, f64)> {
        g.iter()
            .filter_map(|&e| nearby[e].map(|v| (e, v)))
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
    (!fa.is_empty() && !fb.is_empty()).then_some(widest)
}
