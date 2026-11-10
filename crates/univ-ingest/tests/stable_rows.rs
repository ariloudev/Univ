//! What a release keeps from the release before it (build-a-release): the numbers it gave
//! terms, sources and transformation chains, and the object shard of each cell, so that an
//! object the change leaves alone is written as the same rows in the same database.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use release_fixture::{build_next, fixture, loaded_in, read_manifest};
use univ_ingest::release::{BuiltRelease, Kept};
use univ_ingest::state::State;
use univ_model::release::shards::Database;

/// The object shard tables, each with the column naming its object.
const SHARD_TABLES: [(&str, &str); 5] = [
    ("objects", "id"),
    ("entries", "object_id"),
    ("measurements", "object_id"),
    ("designations", "object_id"),
    ("object_cells", "object_id"),
];

/// Every row of `table` in `db` as text, without its release, by the value of `by`.
fn rows_by(db: &rusqlite::Connection, table: &str, by: &str) -> BTreeMap<String, Vec<String>> {
    let mut stmt = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
    let names: Vec<String> = stmt.column_names().map(str::to_owned).collect();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rows = stmt.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let mut key = String::new();
        let mut text = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let value: rusqlite::types::Value = row.get(i).unwrap();
            let shown = format!("{value:?}");
            if name == by {
                key.clone_from(&shown);
            }
            if name != "release" {
                text.push(format!("{name}={shown}"));
            }
        }
        out.entry(key).or_default().push(text.join(" "));
    }
    for each in out.values_mut() {
        each.sort();
    }
    out
}

/// Each object's rows in the object shards of release `r`, with the shard that holds them.
fn object_rows(r: &BuiltRelease) -> BTreeMap<String, BTreeSet<(u32, String)>> {
    let m = read_manifest(&r.dir);
    let mut out: BTreeMap<String, BTreeSet<(u32, String)>> = BTreeMap::new();
    for shard in &m.shards {
        let db = loaded_in(&r.dir, &m, Database::Shard(shard.index));
        for (table, by) in SHARD_TABLES {
            for (object, rows) in rows_by(&db, table, by) {
                let held = out.entry(object).or_default();
                held.extend(
                    rows.into_iter()
                        .map(|t| (shard.index, format!("{table}: {t}"))),
                );
            }
        }
    }
    out
}

/// The catalogue's rows of `table` in release `r`, by the value of `by`.
fn catalogue_rows(r: &BuiltRelease, table: &str, by: &str) -> BTreeMap<String, Vec<String>> {
    let m = read_manifest(&r.dir);
    rows_by(&loaded_in(&r.dir, &m, Database::Catalogue), table, by)
}

/// The catalogue's pairs of a number and what it numbers in release `r`, by `sql`.
fn numbered(r: &BuiltRelease, sql: &str) -> BTreeMap<i64, String> {
    let m = read_manifest(&r.dir);
    let db = loaded_in(&r.dir, &m, Database::Catalogue);
    let mut stmt = db.prepare(sql).unwrap();
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The numbering the state directory `state` keeps for the next release.
fn kept(state: &Path) -> Kept {
    State::read(state).unwrap().kept
}

/// The values both maps hold for each key `a` holds, where they differ.
fn changed<V: PartialEq>(a: &BTreeMap<String, V>, b: &BTreeMap<String, V>) -> Vec<String> {
    a.iter()
        .filter(|(k, v)| b.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect()
}

#[test]
fn a_release_that_only_adds_a_structure_writes_its_objects_as_the_release_before_did() {
    let mut f = fixture();
    let last = f.structures.pop().unwrap();
    let first = build_next(&f);
    let numbering = kept(f.state.path());
    assert!(!numbering.terms.is_empty() && !numbering.cells.is_empty());
    f.structures.push(last);
    let second = build_next(&f);
    assert_eq!(second.number, first.number + 1);

    let (before, after) = (object_rows(&first), object_rows(&second));
    assert!(!before.is_empty());
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    let moved = changed(&before, &after);
    assert!(moved.is_empty(), "objects written differently: {moved:?}");
    for (table, by) in [
        ("terms", "id"),
        ("sources", "n"),
        ("chains", "id"),
        ("names", "object_id"),
        ("object_shards", "id"),
        ("shard_cells", "first"),
    ] {
        let (a, b) = (
            catalogue_rows(&first, table, by),
            catalogue_rows(&second, table, by),
        );
        assert_eq!(a, b, "table {table}");
    }
    assert_eq!(kept(f.state.path()).cells, numbering.cells);

    // It writes no object, entry, measurement, designation or cell of an object, and retires
    // none: every one is the release before's, shared.
    let m = read_manifest(&second.dir);
    assert_eq!(m.parent, Some(first.number));
    for shard in &m.shards {
        for t in &shard.d1.tables {
            assert_eq!(
                (t.added, t.retired),
                (Some(0), Some(0)),
                "shard {} {}",
                shard.index,
                t.table
            );
            if SHARD_TABLES.iter().any(|(name, _)| *name == t.table) {
                let written = m.files.iter().any(|f| {
                    f.path
                        .starts_with(&format!("d1/{}/", Database::Shard(shard.index).dir()))
                        && f.path.contains(&format!("-{}-", t.table))
                });
                assert!(
                    !written,
                    "shard {} writes a file of {}",
                    shard.index, t.table
                );
            }
        }
    }
    // Nor any tile: each is the release before's file, which publishing leaves where it is.
    let tiles = |dir: &Path| -> BTreeMap<String, String> {
        read_manifest(dir)
            .files
            .iter()
            .filter(|f| {
                let bin = Path::new(&f.path)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("bin"));
                f.path.starts_with("tiles/") && bin
            })
            .map(|f| (f.path.clone(), f.sha256.clone()))
            .collect()
    };
    assert!(!tiles(&first.dir).is_empty());
    assert_eq!(tiles(&first.dir), tiles(&second.dir));
    let structures =
        m.d1.tables
            .iter()
            .find(|t| t.table == "structures")
            .unwrap();
    assert!(
        structures.added.unwrap() > 0,
        "the structure added is written"
    );
}

#[test]
fn a_release_that_adds_a_source_keeps_the_numbers_rows_and_shards_of_what_it_leaves_alone() {
    const ADDED: &str = "mcconnachie-2012";
    let mut f = fixture();
    let at_definition = f
        .definitions
        .iter()
        .position(|d| d.id.as_str() == ADDED)
        .unwrap();
    assert!(
        at_definition + 1 < f.definitions.len(),
        "a source in the middle"
    );
    let definition = f.definitions.remove(at_definition);
    let at = f
        .normalized
        .iter()
        .position(|n| {
            n.entries
                .first()
                .is_some_and(|e| e.designation.source.as_str() == ADDED)
        })
        .unwrap();
    let normalized = f.normalized.remove(at);
    let first = build_next(&f);
    // Put back where it was, among the others: the sources after it keep their numbers.
    f.definitions.insert(at_definition, definition);
    f.normalized.push(normalized);
    let second = build_next(&f);

    let added = numbered(&second, "SELECT n, id FROM sources")
        .into_iter()
        .find(|(_, id)| id == ADDED)
        .map(|(n, _)| format!("designations: source=Integer({n}) "))
        .unwrap();
    let (before, after) = (object_rows(&first), object_rows(&second));
    let touched: BTreeSet<&String> = after
        .iter()
        .filter(|(_, rows)| rows.iter().any(|(_, r)| r.starts_with(&added)))
        .map(|(k, _)| k)
        .collect();
    assert!(!touched.is_empty(), "the added source holds objects");
    let all = before.len();
    let left: BTreeMap<String, _> = before
        .into_iter()
        .filter(|(k, _)| !touched.contains(k))
        .collect();
    assert!(left.len() > all / 2);
    let moved = changed(&left, &after);
    assert!(
        moved.is_empty(),
        "objects the source left alone changed: {moved:?}"
    );

    // Every term, source and chain keeps its number; the new ones take numbers never given.
    for sql in [
        "SELECT id, term FROM terms",
        "SELECT n, id FROM sources",
        "SELECT id, steps FROM chains",
    ] {
        let (a, b) = (numbered(&first, sql), numbered(&second, sql));
        for (n, named) in &a {
            if let Some(again) = b.get(n) {
                assert_eq!(again, named, "{sql}: number {n}");
            }
        }
        let highest = a.keys().max().copied().unwrap_or(-1);
        for (n, named) in &b {
            assert!(a.contains_key(n) || *n > highest, "{sql}: {named} took {n}");
        }
    }
    let sources = numbered(&second, "SELECT n, id FROM sources");
    assert!(sources.values().any(|id| id == ADDED));

    // Each source's details stay with it, under its own number.
    let details = |r: &BuiltRelease| {
        numbered(
            r,
            "SELECT s.n, s.id || ' ' || coalesce(d.bibcode, '') || ' ' || coalesce(d.sky, '') \
             FROM sources s JOIN source_details d ON d.source = s.n",
        )
    };
    let (a, b) = (details(&first), details(&second));
    for (n, detail) in &a {
        assert_eq!(b.get(n), Some(detail), "source {n}");
    }
}
