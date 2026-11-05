//! Object ids stay stable from release to release, and the id ledger records every id merged,
//! split or retired (RQ-31, RQ-32).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::collections::BTreeSet;
use std::fs;

use tempfile::TempDir;
use univ_ingest::ids::{
    AssignError, Assigned, EntryIds, Origin, assign, read_ledger, write_ledger,
};
use univ_model::ids::{Designation, ObjectId, SourceId};
use univ_model::ledger::{Change, IdRange, Ledger, LedgerError, Replacement};

/// The entry `key` of a test catalogue.
fn entry(key: &str) -> Designation {
    Designation::new(SourceId::new("test-catalogue").unwrap(), key).unwrap()
}

fn id(n: u64) -> ObjectId {
    ObjectId::new(n).unwrap()
}

fn ids(ns: &[u64]) -> Vec<ObjectId> {
    ns.iter().map(|&n| id(n)).collect()
}

/// Release history for a test: the ledger and the last release's map from entry to id.
struct History {
    ledger: Ledger,
    entries: EntryIds,
    count: usize,
}

impl History {
    fn new() -> Self {
        Self {
            ledger: Ledger::new(),
            entries: EntryIds::new(),
            count: 0,
        }
    }

    /// Makes the next release from `objects`, each a list of entry keys.
    fn release(&mut self, objects: &[&[&str]]) -> Assigned {
        self.count += 1;
        let name = format!("r{}", self.count);
        let groups = objects.iter().map(|o| o.iter().map(|k| entry(k)).collect());
        let assigned = assign(&self.entries, &mut self.ledger, &name, groups).unwrap();
        self.entries = assigned.entries.clone();
        assigned
    }

    /// The id the last release gave the object holding `key`.
    fn id_of(&self, key: &str) -> ObjectId {
        self.entries[&entry(key)]
    }
}

fn replacement(id: u64, release: &str, change: Change, by: &[u64]) -> Replacement {
    Replacement {
        id: ObjectId::new(id).unwrap(),
        release: release.to_owned(),
        change,
        replaced_by: ids(by),
    }
}

#[test]
fn an_object_keeps_its_id_whatever_new_entries_joined_it() {
    // AC-75
    let mut h = History::new();
    h.release(&[&["a1", "a2"], &["b1"]]);
    let (a, b) = (h.id_of("a1"), h.id_of("b1"));
    let r2 = h.release(&[&["a1", "a2", "n1", "n2"], &["b1", "m1"]]);
    assert_eq!((h.id_of("n1"), h.id_of("m1")), (a, b));
    assert!(r2.objects.iter().all(|o| o.origin == Origin::Kept));
    assert_eq!(r2.issued, None);
    assert_eq!(r2.replaced, []);
}

#[test]
fn two_merging_objects_keep_the_older_id_and_the_other_is_merged_into_it() {
    // AC-76. "Older" is the lower id, issued first; here the older object holds the larger entry
    // key, so the order of entries does not decide it.
    let mut h = History::new();
    h.release(&[&["m1"]]);
    h.release(&[&["m1"], &["a1"]]);
    assert_eq!((h.id_of("m1"), h.id_of("a1")), (id(1), id(2)));
    let r3 = h.release(&[&["a1", "m1"]]);
    assert_eq!(h.id_of("a1"), id(1));
    assert_eq!(r3.objects[0].origin, Origin::Merged(ids(&[2])));
    assert_eq!(
        r3.replaced,
        [replacement(2, "r3", Change::MergedInto, &[1])]
    );
    assert_eq!(h.ledger.current(id(2)), BTreeSet::from([id(1)]));
}

#[test]
fn three_objects_merging_keep_the_oldest_id() {
    // AC-76
    let mut h = History::new();
    h.release(&[&["a1"], &["b1", "b2"], &["c1"]]);
    let r2 = h.release(&[&["c1", "b2", "a1", "b1"]]);
    assert_eq!(h.id_of("c1"), id(1));
    assert_eq!(r2.objects[0].origin, Origin::Merged(ids(&[2, 3])));
    assert_eq!(
        r2.replaced,
        [
            replacement(2, "r2", Change::MergedInto, &[1]),
            replacement(3, "r2", Change::MergedInto, &[1]),
        ]
    );
}

#[test]
fn a_split_object_gives_each_part_a_new_id_and_is_recorded_as_split_into_them() {
    // AC-77
    let mut h = History::new();
    h.release(&[&["a1", "a2", "a3"]]);
    let r2 = h.release(&[&["a1"], &["a2", "a3", "n1"]]);
    assert_eq!((h.id_of("a1"), h.id_of("a2")), (id(2), id(3)));
    assert!(r2.objects.iter().all(|o| o.origin == Origin::New));
    assert_eq!(
        r2.issued,
        Some(IdRange {
            first: id(2),
            last: id(3)
        })
    );
    assert_eq!(
        r2.replaced,
        [replacement(1, "r2", Change::SplitInto, &[2, 3])]
    );
    assert_eq!(h.ledger.current(id(1)), BTreeSet::from([id(2), id(3)]));
}

#[test]
fn an_object_that_lost_entries_keeps_its_id_while_the_rest_all_belonged_to_it() {
    // AC-78
    let mut h = History::new();
    h.release(&[&["a1", "a2", "a3"], &["b1", "b2"]]);
    let r2 = h.release(&[&["a1"], &["b1", "n1"]]);
    assert_eq!((h.id_of("a1"), h.id_of("b1")), (id(1), id(2)));
    assert_eq!(r2.replaced, []);
    // Once a lost entry turns up in another object, the object has split.
    let r3 = h.release(&[&["a1"], &["b1"], &["n1"]]);
    assert_eq!(h.id_of("a1"), id(1));
    assert_eq!(
        r3.replaced,
        [replacement(2, "r3", Change::SplitInto, &[3, 4])]
    );
}

#[test]
fn an_object_of_only_new_entries_gets_a_new_id() {
    // AC-79
    let mut h = History::new();
    let r1 = h.release(&[&["b1"], &["a1", "a2"]]);
    // The first release issues ids from 1, in the order of each object's smallest entry.
    assert_eq!((h.id_of("a1"), h.id_of("b1")), (id(1), id(2)));
    assert_eq!(
        r1.issued,
        Some(IdRange {
            first: id(1),
            last: id(2)
        })
    );
    h.release(&[&["a1", "a2"], &["b1"], &["n1", "n2"]]);
    assert_eq!(h.id_of("n2"), id(3));
    assert_eq!(h.ledger.last_issued(), Some(id(3)));
    assert_eq!(h.ledger.issued_in(id(3)), Some("r2"));
}

#[test]
fn an_id_is_never_given_to_a_second_object() {
    // AC-80: the retired id 2 is not given again, not even to the entries that once held it.
    let mut h = History::new();
    h.release(&[&["a1"], &["b1"]]);
    let r2 = h.release(&[&["a1"]]);
    assert_eq!(r2.replaced, [replacement(2, "r2", Change::Retired, &[])]);
    h.release(&[&["a1"], &["b1"], &["c1"]]);
    assert_eq!((h.id_of("b1"), h.id_of("c1")), (id(3), id(4)));
    assert!(h.ledger.is_issued(id(2)) && !h.ledger.is_issued(id(5)));

    // A previous release that still holds a replaced id, or one never issued, is refused.
    let mut stale = h.entries.clone();
    stale.insert(entry("b1"), id(2));
    let refused = assign(&stale, &mut h.ledger, "r4", [vec![entry("b1")]]);
    assert_eq!(refused, Err(AssignError::Replaced(id(2))));
    stale.insert(entry("b1"), id(9));
    let refused = assign(&stale, &mut h.ledger, "r4", [vec![entry("b1")]]);
    assert_eq!(refused, Err(AssignError::NotIssued(id(9))));

    // The ledger takes no range that does not follow on from its high-water mark.
    let before = h.ledger.clone();
    let range = Some(IdRange {
        first: id(2),
        last: id(2),
    });
    let refused = h.ledger.record_release("r4", range, Vec::new());
    assert_eq!(refused, Err(LedgerError::Range("r4".to_owned())));
    assert_eq!(h.ledger, before);
}

#[test]
fn each_ledger_entry_gives_the_old_id_what_happened_and_the_ids_that_replaced_it() {
    // AC-81
    let mut h = History::new();
    h.release(&[&["a1"], &["b1"], &["c1", "c2"], &["d1"]]);
    h.release(&[&["a1", "b1"], &["c1"], &["c2"], &["e1"]]);
    let in_r2: Vec<_> = h.ledger.replaced_in("r2").cloned().collect();
    assert_eq!(
        in_r2,
        [
            replacement(2, "r2", Change::MergedInto, &[1]),
            replacement(3, "r2", Change::SplitInto, &[5, 6]),
            replacement(4, "r2", Change::Retired, &[]),
        ]
    );

    let dir = TempDir::new().unwrap();
    let file = dir.path().join("ids.yaml");
    write_ledger(&file, &h.ledger).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    let expected = "\
version: 1
last_issued: 7
releases:
- name: r1
  issued:
    first: 1
    last: 4
- name: r2
  issued:
    first: 5
    last: 7
replaced:
- id: 2
  release: r2
  change: merged_into
  replaced_by:
  - 1
- id: 3
  release: r2
  change: split_into
  replaced_by:
  - 5
  - 6
- id: 4
  release: r2
  change: retired
  replaced_by: []
";
    assert_eq!(text, expected);
    assert_eq!(read_ledger(&file).unwrap(), h.ledger);
}

#[test]
fn a_merge_and_a_split_in_one_release_resolve_together() {
    let mut h = History::new();
    h.release(&[&["a1", "a2"], &["b1"], &["c1", "c2"]]);
    // A splits; B goes whole into the first part, which keeps B's id; C splits too, so the part
    // holding pieces of A and C only gets a new id.
    let r2 = h.release(&[&["a1", "b1"], &["a2", "c1"], &["c2"]]);
    assert_eq!(h.id_of("b1"), id(2));
    assert_eq!((h.id_of("a2"), h.id_of("c2")), (id(4), id(5)));
    assert_eq!(
        r2.replaced,
        [
            replacement(1, "r2", Change::SplitInto, &[2, 4]),
            replacement(3, "r2", Change::SplitInto, &[4, 5]),
        ]
    );
    // Two objects that trade entries both split, and nothing keeps their ids.
    h.release(&[&["x1", "x2"], &["y1", "y2"]]);
    let (x, y) = (h.id_of("x1"), h.id_of("y1"));
    let r4 = h.release(&[&["x1", "y1"], &["x2", "y2"]]);
    assert!(r4.objects.iter().all(|o| o.origin == Origin::New));
    let changes: Vec<_> = r4.replaced.iter().map(|r| (r.id, r.change)).collect();
    assert!(changes.contains(&(x, Change::SplitInto)) && changes.contains(&(y, Change::SplitInto)));
}

#[test]
fn an_object_whose_entries_all_vanished_is_retired_and_its_id_never_comes_back() {
    let mut h = History::new();
    h.release(&[&["a1", "a2"], &["b1"]]);
    let r2 = h.release(&[]);
    assert_eq!(
        r2.replaced,
        [
            replacement(1, "r2", Change::Retired, &[]),
            replacement(2, "r2", Change::Retired, &[]),
        ]
    );
    assert!(h.ledger.current(id(1)).is_empty());
    h.release(&[&["a1", "a2"], &["b1"]]);
    assert_eq!((h.id_of("a1"), h.id_of("b1")), (id(3), id(4)));
}

#[test]
fn a_ledger_file_that_does_not_stand_is_refused_naming_the_file() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.yaml");
    assert!(read_ledger(&missing).is_err());
    for (name, text, problem) in [
        ("version", "version: 2\nlast_issued: 0\n", "version 2"),
        (
            "high-water",
            "version: 1\nlast_issued: 3\nreleases:\n- name: r1\n  issued: { first: 1, last: 2 }\n",
            "last id issued is 3",
        ),
        (
            "never-issued",
            "version: 1\nlast_issued: 1\nreleases:\n- name: r1\n  issued: { first: 1, last: 1 }\n\
             - name: r2\nreplaced:\n- { id: 1, release: r2, change: merged_into, replaced_by: [2] }\n",
            "id 2 was never issued",
        ),
        (
            "split-into-one",
            "version: 1\nlast_issued: 2\nreleases:\n- name: r1\n  issued: { first: 1, last: 1 }\n\
             - name: r2\n  issued: { first: 2, last: 2 }\n\
             replaced:\n- { id: 1, release: r2, change: split_into, replaced_by: [2] }\n",
            "SplitInto with 1",
        ),
    ] {
        let file = dir.path().join(format!("{name}.yaml"));
        fs::write(&file, text).unwrap();
        let error = read_ledger(&file).unwrap_err().to_string();
        assert!(error.contains(&format!("{name}.yaml")), "{error}");
        assert!(error.contains(problem), "{name}: {error}");
    }
}

#[test]
fn an_object_without_entries_or_an_entry_in_two_objects_is_refused() {
    let mut ledger = Ledger::new();
    let none = EntryIds::new();
    let empty = assign(&none, &mut ledger, "r1", [vec![entry("a1")], vec![]]);
    assert_eq!(empty, Err(AssignError::EmptyObject(1)));
    let twice = assign(
        &none,
        &mut ledger,
        "r1",
        [vec![entry("a1")], vec![entry("b1"), entry("a1")]],
    );
    assert_eq!(twice, Err(AssignError::EntryInTwoObjects(entry("a1"))));
    assert_eq!(ledger, Ledger::new());
}
