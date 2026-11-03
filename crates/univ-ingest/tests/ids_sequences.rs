//! Object ids over many random release sequences: the same groups give the same ids in any
//! order, no id is given to a second object, and every id that leaves has its ledger entry
//! (RQ-31, RQ-32).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::collections::{BTreeMap, BTreeSet};

use univ_ingest::ids::{Assigned, EntryIds, Origin, assign};
use univ_model::ids::{Designation, ObjectId, SourceId};
use univ_model::ledger::{Change, Ledger};

/// `SplitMix64`: a small seeded generator, so each run draws the same sequences.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// Entry `n` of one of two test catalogues.
fn entry(n: usize) -> Designation {
    let source = if n.is_multiple_of(3) {
        "cat-a"
    } else {
        "cat-b"
    };
    Designation::new(SourceId::new(source).unwrap(), format!("{n:03}")).unwrap()
}

/// The next release's grouping: the previous objects less the entries that vanished, new entries
/// alone, then random merges, splits and moves.
fn next_grouping(rng: &mut Rng, previous: &[Vec<usize>], universe: usize) -> Vec<Vec<usize>> {
    let held: BTreeSet<usize> = previous.iter().flatten().copied().collect();
    let mut groups: Vec<Vec<usize>> = previous
        .iter()
        .map(|g| g.iter().copied().filter(|_| !rng.chance(10)).collect())
        .collect();
    groups.extend(
        (0..universe)
            .filter(|n| !held.contains(n) && rng.chance(15))
            .map(|n| vec![n]),
    );
    for _ in 0..rng.below(6) {
        groups.retain(|g| !g.is_empty());
        if groups.len() < 2 {
            break;
        }
        let a = rng.below(groups.len());
        match rng.below(3) {
            0 => {
                let taken = groups.swap_remove(a);
                let b = rng.below(groups.len());
                groups[b].extend(taken);
            }
            1 if groups[a].len() > 1 => {
                rng.shuffle(&mut groups[a]);
                let cut = 1 + rng.below(groups[a].len() - 1);
                let part = groups[a].split_off(cut);
                groups.push(part);
            }
            _ => {
                let b = rng.below(groups.len());
                let moved = groups[a].pop().unwrap();
                groups[b].push(moved);
            }
        }
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// `groups` as entries, shuffled within and between objects.
fn shuffled(rng: &mut Rng, groups: &[Vec<usize>]) -> Vec<Vec<Designation>> {
    let mut objects: Vec<Vec<Designation>> = groups
        .iter()
        .map(|g| g.iter().map(|&n| entry(n)).collect())
        .collect();
    for object in &mut objects {
        rng.shuffle(object);
    }
    rng.shuffle(&mut objects);
    objects
}

#[test]
fn the_same_groups_give_the_same_ids_in_any_order() {
    let mut rng = Rng(7);
    let mut ledger = Ledger::new();
    let mut previous = EntryIds::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for release in 1..=6 {
        groups = next_grouping(&mut rng, &groups, 40);
        let name = format!("r{release}");
        let mut outcomes = Vec::new();
        for _ in 0..5 {
            let mut copy = ledger.clone();
            let objects = shuffled(&mut rng, &groups);
            let assigned = assign(&previous, &mut copy, &name, objects).unwrap();
            outcomes.push((assigned, copy));
        }
        assert!(outcomes.windows(2).all(|w| w[0] == w[1]), "release {name}");
        let (assigned, next) = outcomes.swap_remove(0);
        ledger = next;
        previous = assigned.entries;
    }
}

/// Checks one release of a sequence against the release before it.
fn check_release(
    ledger: &Ledger,
    name: &str,
    before: &EntryIds,
    high_water: u64,
    ever: &BTreeSet<ObjectId>,
    assigned: &Assigned,
) {
    let live_before: BTreeSet<ObjectId> = before.values().copied().collect();
    let live: BTreeSet<ObjectId> = assigned.objects.iter().map(|o| o.id).collect();
    assert_eq!(
        live.len(),
        assigned.objects.len(),
        "{name}: an id given twice"
    );
    for object in &assigned.objects {
        let id = object.id;
        if live_before.contains(&id) {
            assert_ne!(object.origin, Origin::New, "{name}: {id}");
        } else {
            // A new id: above the high-water mark, so never given before.
            assert!(
                id.get() > high_water && !ever.contains(&id),
                "{name}: {id} reused"
            );
            assert_eq!(object.origin, Origin::New);
        }
        // AC-75 and AC-78: carried-over entries all from one previous object that went whole.
        let from: BTreeSet<ObjectId> = object
            .entries
            .iter()
            .filter_map(|e| before.get(e))
            .copied()
            .collect();
        if let [only] = from.iter().copied().collect::<Vec<_>>()[..] {
            let whole = before
                .iter()
                .filter(|&(_, &was)| was == only)
                .all(|(e, _)| !assigned.entries.contains_key(e) || object.entries.contains(e));
            assert_eq!(whole, id == only, "{name}: {only} and {id}");
        }
    }
    // Every id that left has its ledger entry from this release, and no live id has one.
    for &old in &live_before {
        let replacement = ledger.replacement(old);
        if live.contains(&old) {
            assert!(replacement.is_none(), "{name}: live {old} replaced");
        } else {
            let r = replacement.unwrap_or_else(|| unreachable!("{name}: {old} left unrecorded"));
            assert_eq!(r.release, name);
            assert!(r.replaced_by.iter().all(|by| live.contains(by)));
            if r.change == Change::MergedInto {
                assert!(
                    r.replaced_by[0] < old,
                    "{name}: {old} merged into a newer id"
                );
            }
        }
    }
    let replaced: Vec<ObjectId> = ledger.replaced_in(name).map(|r| r.id).collect();
    assert!(
        replaced
            .iter()
            .all(|id| live_before.contains(id) && !live.contains(id))
    );
}

#[test]
fn over_many_random_release_sequences_no_id_is_reused_and_every_old_id_is_recorded() {
    let mut rng = Rng(2026);
    let mut counts: BTreeMap<Change, usize> = BTreeMap::new();
    for _ in 0..200 {
        let mut ledger = Ledger::new();
        let mut previous = EntryIds::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut ever: BTreeSet<ObjectId> = BTreeSet::new();
        for release in 1..=10 {
            groups = next_grouping(&mut rng, &groups, 30);
            let name = format!("r{release}");
            let high_water = ledger.last_issued().map_or(0, ObjectId::get);
            let objects = shuffled(&mut rng, &groups);
            let assigned = assign(&previous, &mut ledger, &name, objects).unwrap();
            check_release(&ledger, &name, &previous, high_water, &ever, &assigned);
            ever.extend(assigned.objects.iter().map(|o| o.id));
            assert_eq!(ledger.last_issued(), ever.last().copied());
            for r in ledger.replaced_in(&name) {
                *counts.entry(r.change).or_default() += 1;
            }
            previous = assigned.entries;
        }
        // Every id ever given is live or has its ledger entry, and the ledger reads back whole.
        let live: BTreeSet<ObjectId> = previous.values().copied().collect();
        assert!(
            ever.iter()
                .all(|id| live.contains(id) ^ ledger.replacement(*id).is_some())
        );
        let yaml = serde_norway::to_string(&ledger).unwrap();
        assert_eq!(serde_norway::from_str::<Ledger>(&yaml).unwrap(), ledger);
    }
    // The sequences exercised every change.
    assert!(
        counts.values().all(|&n| n > 50) && counts.len() == 3,
        "{counts:?}"
    );
}
