//! A release divided among the catalogue database and object shards (build-a-release): the
//! fixture's objects, under the fixture's small shard budget, span more than one shard.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::collections::{BTreeMap, BTreeSet};

use release_fixture::{SHARD_BUDGET, build_next, built, cross, fixture, loaded_in, tiles};
use rusqlite::Connection;
use univ_ingest::release::{BuildError, LOOSE_BUCKETS};
use univ_model::release::ReleaseManifest;
use univ_model::release::shards::{Database, SHARD_TABLES};

fn ids(db: &Connection, sql: &str) -> BTreeSet<u64> {
    let mut statement = db.prepare(sql).unwrap();
    statement
        .query_map([], |r| r.get::<_, u64>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn count(db: &Connection, table: &str) -> u64 {
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// The fixture's release 1 with its catalogue database and each object shard loaded apart.
struct Sharded {
    manifest: ReleaseManifest,
    catalogue: Connection,
    shards: Vec<Connection>,
}

fn sharded() -> Sharded {
    let f = fixture();
    let release = build_next(&f);
    let m = release.manifest;
    assert!(m.shards.len() >= 2, "{} shard(s)", m.shards.len());
    let catalogue = loaded_in(&release.dir, &m, Database::Catalogue);
    let shards = m
        .shards
        .iter()
        .map(|s| loaded_in(&release.dir, &m, Database::Shard(s.index)))
        .collect();
    Sharded {
        manifest: m,
        catalogue,
        shards,
    }
}

#[test]
fn each_object_is_in_exactly_one_shard_with_its_entries_measurements_designations_and_cell() {
    let s = sharded();
    let index: BTreeMap<u64, u64> = {
        let mut q = s
            .catalogue
            .prepare("SELECT id, shard FROM object_shards WHERE release = 1")
            .unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(index.len() as u64, s.manifest.counts.objects);
    let mut seen = BTreeSet::new();
    for (i, shard) in s.shards.iter().enumerate() {
        let objects = ids(shard, "SELECT id FROM objects");
        for id in &objects {
            assert!(seen.insert(*id), "object {id} is in two shards");
            assert_eq!(index[id], i as u64, "object {id}");
        }
        for table in SHARD_TABLES.iter().filter(|t| **t != "objects") {
            let owners = ids(shard, &format!("SELECT DISTINCT object_id FROM {table}"));
            assert!(owners.is_subset(&objects), "{table} of shard {i}");
        }
        assert_eq!(objects.len() as u64, s.manifest.shards[i].objects);
    }
    assert_eq!(seen.len(), index.len());
    // The catalogue holds no object's rows.
    for table in SHARD_TABLES {
        assert_eq!(count(&s.catalogue, table), 0, "{table}");
    }
}

#[test]
fn all_the_objects_of_one_cell_are_in_one_shard_and_one_not_placed_goes_by_its_id() {
    let s = sharded();
    let mut shard_of_cell: BTreeMap<(i64, i64, i64, i64), usize> = BTreeMap::new();
    let mut placed: BTreeSet<u64> = BTreeSet::new();
    for (i, shard) in s.shards.iter().enumerate() {
        let mut q = shard
            .prepare("SELECT domain, ix, iy, iz, object_id FROM object_cells")
            .unwrap();
        let rows = q
            .query_map([], |r| {
                Ok(((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?), r.get(4)?))
            })
            .unwrap()
            .map(Result::unwrap);
        for (cell, id) in rows {
            placed.insert(id);
            let owner = *shard_of_cell.entry(cell).or_insert(i);
            assert_eq!(owner, i, "cell {cell:?} is in two shards");
        }
    }
    // The objects not placed whose ids share a bucket share a shard.
    let mut shard_of_bucket = BTreeMap::new();
    let mut loose = 0;
    for (i, shard) in s.shards.iter().enumerate() {
        for id in ids(shard, "SELECT id FROM objects") {
            if !placed.contains(&id) {
                loose += 1;
                let owner = *shard_of_bucket.entry(id % LOOSE_BUCKETS).or_insert(i);
                assert_eq!(owner, i, "object {id}");
            }
        }
    }
    assert!(loose > 0, "the fixture holds objects that are not placed");
    let manifest_loose: u64 = s.manifest.shards.iter().map(|s| s.unplaced).sum();
    assert_eq!(manifest_loose, loose);
}

#[test]
fn the_catalogue_holds_the_sources_terms_structures_and_the_shard_of_every_object() {
    let s = sharded();
    for table in [
        "sources",
        "terms",
        "structures",
        "scale_domains",
        "releases",
        "source_reading",
    ] {
        assert!(count(&s.catalogue, table) > 0, "{table}");
        for shard in &s.shards {
            assert_eq!(count(shard, table), 0, "{table} in a shard");
        }
    }
    // Every database holds the whole schema and its version.
    for db in s.shards.iter().chain([&s.catalogue]) {
        let version: i64 = db
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, i64::from(univ_model::release::SCHEMA_VERSION));
    }
    assert_eq!(
        count(&s.catalogue, "object_shards"),
        s.manifest.counts.objects
    );
}

#[test]
fn the_manifest_names_each_shard_with_its_cells_and_estimated_size() {
    let s = sharded();
    for (i, shard) in s.manifest.shards.iter().enumerate() {
        assert_eq!(shard.index as usize, i);
        assert!(shard.d1.estimated_bytes > 0);
        assert!(shard.d1.estimated_bytes <= SHARD_BUDGET, "shard {i}");
        let rows: u64 = s
            .catalogue
            .query_row(
                "SELECT estimated_bytes FROM release_shards WHERE release = 1 AND shard = ?1",
                [i],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, shard.d1.estimated_bytes, "shard {i}");
        let runs: u64 = s
            .catalogue
            .query_row(
                "SELECT COUNT(*) FROM shard_cells WHERE release = 1 AND shard = ?1",
                [i],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(runs, shard.cells.len() as u64, "shard {i}");
        for run in &shard.cells {
            assert!(run.first <= run.last && run.cells > 0, "{run:?}");
        }
    }
    // The runs of one domain follow one another without overlapping.
    let mut by_domain: BTreeMap<&str, Vec<(u64, u64)>> = BTreeMap::new();
    for run in s.manifest.shards.iter().flat_map(|s| &s.cells) {
        by_domain
            .entry(run.domain.as_str())
            .or_default()
            .push((run.first, run.last));
    }
    for (domain, runs) in by_domain {
        for pair in runs.windows(2) {
            assert!(pair[0].1 < pair[1].0, "{domain}: {pair:?}");
        }
    }
}

#[test]
fn a_cell_beyond_a_shards_budget_refuses_the_build_naming_it_and_uses_no_number() {
    let f = fixture();
    let mut crossed = cross(&f);
    let tiles = tiles(&f, &crossed);
    crossed.basis.config.shard_budget_bytes = Some(100);
    let err = built(&f, &crossed, &tiles, &f.definitions, &f.structures).unwrap_err();
    let BuildError::Release {
        number: Some(1),
        problem,
    } = &err
    else {
        panic!("{err}");
    };
    assert!(problem.starts_with("cell ("), "{problem}");
    assert!(problem.contains(" objects, estimated at "), "{problem}");
    assert!(
        problem.ends_with("more than an object shard's budget of 100 bytes"),
        "{problem}"
    );
    assert!(err.to_string().contains("its number is still free"));
    // The next build is release 1 all the same.
    assert_eq!(build_next(&f).number, 1);
}

#[test]
fn the_catalogue_keeps_once_for_each_source_its_designation_form_flag_meanings_and_rank() {
    let s = sharded();
    let reading = |id: &str| -> (Option<String>, Option<u32>, Option<String>) {
        s.catalogue
            .query_row(
                "SELECT r.designation_form, r.rank, r.flags FROM source_reading r \
                 JOIN sources s ON s.n = r.source WHERE s.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    };
    // As sources/hipparcos-2007.yaml and rules/designations.yaml give them.
    let (form, rank, flags) = reading("hipparcos-2007");
    assert_eq!(form.as_deref(), Some("HIP {identifier}"));
    assert_eq!(rank, Some(1));
    let flags: serde_json::Value = serde_json::from_str(&flags.unwrap()).unwrap();
    assert!(flags["Sn"]["values"]["5"].is_string(), "{flags}");
    assert!(flags["Sn"]["cite"].is_string(), "{flags}");
    // A source ranked but with no form of its own: its names are already its designations.
    let (form, rank, _) = reading("mcconnachie-2012");
    assert_eq!((form, rank.is_some()), (None, true));
    // Once each.
    let rows: u64 = s
        .catalogue
        .query_row(
            "SELECT COUNT(*) - COUNT(DISTINCT source) FROM source_reading",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}
