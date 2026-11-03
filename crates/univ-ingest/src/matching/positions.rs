//! The second pass: the groups the cross-identifications left, matched by position and by
//! redshift or distance. A group with more than one candidate from one source is linked to each
//! and merged with none; every other candidate is merged, in falling confidence, unless the
//! merge would put two entries of one source or two linked groups together.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::index::SkyIndex;
use super::rules::{Comparison, key};
use super::{Edge, Join, Work};

/// A group of entries after the first pass: its members and their sources.
struct Group {
    members: Vec<usize>,
    sources: Vec<u16>,
    positioned: bool,
}

/// A candidate match between the groups `i` and `j`, through their entries `a` and `b`.
struct Candidate {
    i: usize,
    j: usize,
    a: usize,
    b: usize,
    comparison: Comparison,
}

pub(super) fn apply(work: &mut Work<'_>) {
    let groups = positioned_groups(work);
    if groups.is_empty() || work.rules.pairs.is_empty() {
        return;
    }
    let candidates = candidates(work, &groups);
    let mut by_group: Vec<Vec<usize>> = vec![Vec::new(); groups.len()];
    for (c, cand) in candidates.iter().enumerate() {
        by_group[cand.i].push(c);
        by_group[cand.j].push(c);
    }
    // Two candidates of one group compete when the far groups share a source.
    let shares = |x: usize, y: usize| {
        groups[x]
            .sources
            .iter()
            .any(|s| groups[y].sources.contains(s))
    };
    let far = |c: &Candidate, g: usize| if c.i == g { c.j } else { c.i };
    let ambiguous = |c: &Candidate| {
        [c.i, c.j].into_iter().any(|g| {
            let other = far(c, g);
            by_group[g].iter().any(|&d| {
                let o = far(&candidates[d], g);
                o != other && shares(o, other)
            })
        })
    };
    let (linked, mut clear): (Vec<&Candidate>, Vec<&Candidate>) =
        candidates.iter().partition(|c| ambiguous(c));
    for c in linked {
        work.groups.link(c.a, c.b);
        work.links.push((edge(c), c.comparison.confidence));
    }
    clear.sort_by(|x, y| {
        y.comparison
            .confidence
            .partial_cmp(&x.comparison.confidence)
            .unwrap_or(Ordering::Equal)
            .then((x.i, x.j).cmp(&(y.i, y.j)))
    });
    for c in clear {
        if work.groups.join(c.a, c.b).is_ok() {
            work.joins.push(edge(c));
        } else {
            work.groups.link(c.a, c.b);
            work.links.push((edge(c), c.comparison.confidence));
        }
    }
}

fn edge(c: &Candidate) -> Edge {
    Edge {
        a: c.a,
        b: c.b,
        join: Join::Position(c.comparison),
    }
}

/// The groups holding at least one entry with a position.
fn positioned_groups(work: &mut Work<'_>) -> Vec<Group> {
    let n = work.entries.len();
    let mut slot = vec![usize::MAX; n];
    let mut groups: Vec<Group> = Vec::new();
    for e in 0..n {
        let root = work.groups.find(e);
        if slot[root] == usize::MAX {
            slot[root] = groups.len();
            groups.push(Group {
                members: Vec::new(),
                sources: Vec::new(),
                positioned: false,
            });
        }
        let g = &mut groups[slot[root]];
        g.members.push(e);
        g.sources.push(work.source_of[e]);
        g.positioned |= work.entries[e].position.is_some();
    }
    groups.retain(|g| g.positioned);
    groups
}

/// Every pair of groups with no source in common that some pair rule finds within its
/// tolerances, through the pair of their entries with the highest confidence. Every entry with
/// a position is indexed, so the search radius is the largest tolerance and no more.
fn candidates(work: &Work<'_>, groups: &[Group]) -> Vec<Candidate> {
    let rules = work.rules;
    let pairs = rules.pair_index();
    let mut group_of = Vec::new();
    let mut entry_of = Vec::new();
    let mut positions = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        for &e in &group.members {
            if let Some(p) = &work.entries[e].position {
                group_of.push(g);
                entry_of.push(e);
                positions.push((p.icrs.ra, p.icrs.dec));
            }
        }
    }
    let index = SkyIndex::new(&positions, rules.search_radius_degrees());
    let mut near = Vec::new();
    let mut found = Vec::new();
    // The best comparison with each later group, for the group in hand.
    let mut best: BTreeMap<usize, (usize, usize, Comparison)> = BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        best.clear();
        for &a in &g.members {
            let Some(at) = &work.entries[a].position else {
                continue;
            };
            index.near(at.icrs.ra, at.icrs.dec, &mut near);
            for &found_at in &near {
                let (j, b) = (group_of[found_at as usize], entry_of[found_at as usize]);
                let h = &groups[j];
                if j <= i || g.sources.iter().any(|s| h.sources.contains(s)) {
                    continue;
                }
                let (sa, sb) = (work.source_of[a], work.source_of[b]);
                let names = (&work.sources[sa as usize], &work.sources[sb as usize]);
                let Some(pair) = pairs.get(&key(names.0, names.1)) else {
                    continue;
                };
                let Some(c) = rules.compare(pair, &work.entries[a], &work.entries[b]) else {
                    continue;
                };
                let slot = best.entry(j).or_insert((a, b, c));
                if c.confidence > slot.2.confidence {
                    *slot = (a, b, c);
                }
            }
        }
        found.extend(best.iter().map(|(&j, &(a, b, comparison))| Candidate {
            i,
            j,
            a,
            b,
            comparison,
        }));
    }
    found
}
