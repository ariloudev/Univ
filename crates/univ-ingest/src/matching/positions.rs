//! The second pass: the groups the cross-identifications left, matched by position and by
//! redshift or distance. A group with more than one candidate from one source is linked to each
//! and merged with none; every other candidate is merged, in falling confidence, unless the
//! merge would put two entries of one source or two linked groups together, or would put an
//! entry beside one that a pair rule compares it with and finds beyond its tolerances (complete
//! linkage): that candidate becomes a link instead, so that no chain of close pairs joins two
//! entries that are far apart. Entries a published cross-identification joined, such as a Local
//! Group dwarf's two catalogue centres arcminutes apart, are one body: complete linkage asks an
//! entry to lie within the tolerances of one of them, not of each.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use super::index::SkyIndex;
use super::rules::Comparison;
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
    let mut members = Members::new(work, &groups);
    let units = units(work);
    for c in clear {
        let (ra, rb) = (work.groups.find(c.a), work.groups.find(c.b));
        if members.compatible(work, &units, ra, rb) && work.groups.join(c.a, c.b).is_ok() {
            members.joined(ra, rb, work.groups.find(c.a));
            work.joins.push(edge(c));
        } else {
            work.groups.link(c.a, c.b);
            work.links.push((edge(c), c.comparison.confidence));
        }
    }
}

/// The entries of each group of more than one entry, by its root; a group of one entry is
/// its root alone.
struct Members(HashMap<usize, Vec<usize>>);

impl Members {
    fn new(work: &mut Work<'_>, groups: &[Group]) -> Self {
        let mut by_root = HashMap::new();
        for g in groups.iter().filter(|g| g.members.len() > 1) {
            by_root.insert(work.groups.find(g.members[0]), g.members.clone());
        }
        Self(by_root)
    }

    fn of<'a>(&'a self, root: &'a usize) -> &'a [usize] {
        self.0
            .get(root)
            .map_or(std::slice::from_ref(root), Vec::as_slice)
    }

    /// Whether the groups rooted at `ra` and `rb` may be one: for every body of the one and
    /// every body of the other (`units`: entries a published cross-identification joined are
    /// one body), some pair of their entries that a pair rule compares lies within its
    /// tolerances, or no pair is compared. A body of one entry each is complete linkage.
    fn compatible(&self, work: &Work<'_>, units: &[u32], ra: usize, rb: usize) -> bool {
        let (a, b) = (self.of(&ra), self.of(&rb));
        let bodies = |side: &[usize]| {
            let mut u: Vec<u32> = side.iter().map(|&e| units[e]).collect();
            u.sort_unstable();
            u.dedup();
            u
        };
        let (bodies_a, bodies_b) = (bodies(a), bodies(b));
        bodies_a.iter().all(|&ua| {
            bodies_b.iter().all(|&ub| {
                let mut compared = false;
                for &x in a.iter().filter(|&&x| units[x] == ua) {
                    for &y in b.iter().filter(|&&y| units[y] == ub) {
                        if work.pair(x, y).is_some()
                            && work.units[x].is_some()
                            && work.units[y].is_some()
                        {
                            if work.compare(x, y).is_some() {
                                return true;
                            }
                            compared = true;
                        }
                    }
                }
                !compared
            })
        })
    }

    /// Records that the groups rooted at `ra` and `rb` are now one, rooted at `root`.
    fn joined(&mut self, ra: usize, rb: usize, root: usize) {
        let mut all = self.of(&ra).to_vec();
        all.extend_from_slice(self.of(&rb));
        self.0.remove(&ra);
        self.0.remove(&rb);
        self.0.insert(root, all);
    }
}

/// Each entry's body: the least entry joined to it by published cross-identifications, the only
/// joins made before this pass, or itself.
fn units(work: &Work<'_>) -> Vec<u32> {
    let mut parent: Vec<u32> = (0..work.entries.len())
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        .collect();
    for e in work
        .joins
        .iter()
        .filter(|e| matches!(e.join, Join::Cross { .. }))
    {
        let (Ok(a), Ok(b)) = (u32::try_from(e.a), u32::try_from(e.b)) else {
            continue;
        };
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        parent[ra.max(rb) as usize] = ra.min(rb);
    }
    (0..parent.len())
        .map(|i| root(&mut parent, u32::try_from(i).unwrap_or(u32::MAX)))
        .collect()
}

/// The root of `i` in the forest `parent`, halving the path on the way.
fn root(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        let up = parent[parent[i as usize] as usize];
        parent[i as usize] = up;
        i = up;
    }
    i
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
    let index = SkyIndex::new(&positions, work.rules.search_radius_degrees());
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
                let Some(c) = work.compare(a, b) else {
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
