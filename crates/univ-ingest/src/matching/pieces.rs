//! The third pass: the pieces of each nearby galaxy that publishes its extent. Galaxies are taken
//! largest first; every group the earlier passes left whose entries all come from a piece
//! source, all lie within the galaxy's extent and all agree with its velocity joins the galaxy,
//! unless the two hold entries of one identity source, such as two PGC numbers, or another such
//! group holds an entry of the same identity source: those are linked to it as candidates.

use std::collections::{HashMap, HashSet};

use univ_model::object::{LineOfSight, MatchMethod};
use univ_model::position::separation_degrees;

use super::extent::ExtentRule;
use super::index::SkyIndex;
use super::rules::Comparison;
use super::{Edge, Join, Work};

/// A galaxy that takes pieces: its entry, the radius of its extent in degrees, its cz.
struct Galaxy {
    entry: usize,
    radius: f64,
    velocity: f64,
}

/// The entries of each group: the groups as the earlier passes left them, sorted by root, and
/// those this pass has joined since, by their new root.
pub(super) struct Members {
    base: Vec<(usize, usize)>,
    pub(super) joined: HashMap<usize, Vec<usize>>,
}

impl Members {
    pub(super) fn new(work: &mut Work<'_>) -> Self {
        let mut base: Vec<(usize, usize)> = (0..work.entries.len())
            .map(|e| (work.groups.find(e), e))
            .collect();
        base.sort_unstable();
        Self {
            base,
            joined: HashMap::new(),
        }
    }

    pub(super) fn of(&self, root: usize) -> Vec<usize> {
        if let Some(m) = self.joined.get(&root) {
            return m.clone();
        }
        let from = self.base.partition_point(|&(r, _)| r < root);
        let to = self.base.partition_point(|&(r, _)| r <= root);
        self.base[from..to].iter().map(|&(_, e)| e).collect()
    }
}

/// What the pass did: the number of groups joined to a galaxy as its pieces.
pub(super) fn apply(work: &mut Work<'_>) -> u64 {
    let Some(rule) = work.rules.extents.as_ref() else {
        return 0;
    };
    let galaxies = galaxies(work, rule);
    let Some(widest) = galaxies.first().map(|g| g.radius) else {
        return 0;
    };
    let in_list = |list: &[univ_model::ids::SourceId]| -> Vec<bool> {
        work.sources.iter().map(|s| list.contains(s)).collect()
    };
    let (piece_source, identity) = (in_list(&rule.pieces), in_list(&rule.identities));
    let mut at = Vec::new();
    let mut positions = Vec::new();
    for (e, entry) in work.entries.iter().enumerate() {
        if let Some(p) = &entry.position
            && piece_source[usize::from(work.source_of[e])]
        {
            at.push(e);
            positions.push((p.icrs.ra, p.icrs.dec));
        }
    }
    let index = SkyIndex::new(&positions, widest);
    let mut members = Members::new(work);
    let mut near = Vec::new();
    let mut joined = 0;
    for g in &galaxies {
        let Some(centre) = work.entries[g.entry].position.as_ref().map(|p| p.icrs) else {
            continue;
        };
        index.near(centre.ra, centre.dec, &mut near);
        let home = work.groups.find(g.entry);
        let host = members.of(home);
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        let middle = centre.unit_vector();
        for &k in &near {
            let e = at[k as usize];
            if work.units[e].is_none_or(|u| separation_degrees(middle, u) > g.radius) {
                continue;
            }
            let root = work.groups.find(e);
            if root == home || !seen.insert(root) {
                continue;
            }
            let piece = members.of(root);
            if let Some((via, c)) = compare(work, rule, g, &host, &piece, &piece_source, &identity)
            {
                found.push((root, piece, via, c));
            }
        }
        // Two pieces holding entries of one identity source are two galaxies, and neither is
        // known to be this one: both stay apart, linked to it as candidates.
        let held = |piece: &[usize], s: u16| piece.iter().any(|&e| work.source_of[e] == s);
        let claims: Vec<u16> = found
            .iter()
            .flat_map(|(_, piece, _, _)| piece.iter().map(|&e| work.source_of[e]))
            .filter(|&s| identity[usize::from(s)])
            .collect();
        let contested = |piece: &[usize]| {
            claims
                .iter()
                .any(|&s| held(piece, s) && found.iter().filter(|f| held(&f.1, s)).count() > 1)
        };
        let (taken, refused): (Vec<_>, Vec<_>) = found.iter().partition(|f| !contested(&f.1));
        for (_, _, via, c) in refused {
            work.groups.link(g.entry, *via);
            work.links.push((edge(g.entry, *via, *c), c.confidence));
        }
        let mut all = host;
        for (root, piece, via, c) in taken {
            work.groups.absorb(g.entry, *via);
            members.joined.remove(root);
            all.extend_from_slice(piece);
            work.joins.push(edge(g.entry, *via, *c));
            joined += 1;
        }
        members.joined.remove(&home);
        members.joined.insert(work.groups.find(g.entry), all);
    }
    joined
}

fn edge(a: usize, b: usize, c: Comparison) -> Edge {
    Edge {
        a,
        b,
        join: Join::Position(c),
    }
}

/// The galaxies that publish an extent and a cz below the rule's, largest first.
fn galaxies(work: &Work<'_>, rule: &ExtentRule) -> Vec<Galaxy> {
    let mut found: Vec<Galaxy> = work
        .entries
        .iter()
        .enumerate()
        .filter(|(e, _)| work.units[*e].is_some())
        .filter_map(|(entry, e)| {
            let radius = rule.radius_degrees(e)?;
            let velocity = work.rules.velocity(e)?;
            (velocity < rule.below_km_s).then_some(Galaxy {
                entry,
                radius,
                velocity,
            })
        })
        .collect();
    found.sort_by(|a, b| b.radius.total_cmp(&a.radius).then(a.entry.cmp(&b.entry)));
    found
}

/// Whether the group of `piece` joins the galaxy `g`, whose group is `host`: through its entry
/// nearest the galaxy's centre, with the separation and the largest velocity difference.
fn compare(
    work: &Work<'_>,
    rule: &ExtentRule,
    g: &Galaxy,
    host: &[usize],
    piece: &[usize],
    piece_source: &[bool],
    identity: &[bool],
) -> Option<(usize, Comparison)> {
    let source = |e: usize| usize::from(work.source_of[e]);
    if piece.iter().any(|&e| !piece_source[source(e)]) {
        return None;
    }
    if piece.iter().any(|&e| {
        identity[source(e)] && host.iter().any(|&h| work.source_of[h] == work.source_of[e])
    }) {
        return None;
    }
    let centre = work.units[g.entry]?;
    let mut nearest: Option<(usize, f64)> = None;
    let mut delta: Option<f64> = None;
    for &e in piece {
        let Some(u) = work.units[e] else { continue };
        let s = separation_degrees(centre, u);
        if s > g.radius {
            return None;
        }
        if nearest.is_none_or(|(_, n)| s < n) {
            nearest = Some((e, s));
        }
        // Every entry of a piece carries a cz agreeing with the galaxy's: one without a cz could
        // be anything on the line of sight.
        let d = (work.rules.velocity(&work.entries[e])? - g.velocity).abs();
        if d > rule.tolerance_km_s {
            return None;
        }
        delta = Some(delta.map_or(d, |m: f64| m.max(d)));
    }
    let ((via, s), delta) = (nearest?, delta?);
    let p = work.rules.confidence.exponent;
    let factor = 1.0 - (delta / rule.tolerance_km_s).powf(p);
    Some((
        via,
        Comparison {
            method: MatchMethod::WithinExtent,
            separation_arcsec: s * 3_600.0,
            difference: Some(LineOfSight::Velocity { delta_km_s: delta }),
            confidence: work.rules.positional_confidence(s, g.radius, factor),
        },
    ))
}
