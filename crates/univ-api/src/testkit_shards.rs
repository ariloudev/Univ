//! The test backend's object shards: in-memory SQLite databases beside the catalogue, each with
//! the release schema, shared with the catalogue's connection by name so that it can move rows
//! into them.
//!
//! A fixture writes every row to the catalogue. Its rows of the shard tables are then moved to
//! the shard the catalogue's `object_shards` names for their object, as a published release has
//! them; an object it names no shard for is given one by the backend's placement, `id % 2`
//! unless a test sets another. The index is then written as a build writes it, from what the
//! shards hold: each shard's objects (`release_shards`) and the range of the Morton codes of its
//! cells of each domain (`shard_cells`), each row added by the first release that has it and
//! shared by every later one; an object's row of the index is retired with its object. A fixture written by hand thus spans two shards, and
//! every read is tested across them.

use std::cell::Cell as Shared;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OpenFlags};
use univ_model::release::shards::{SHARD_TABLES, morton};
use univ_model::release::{Cell, SCHEMA};

/// The shards of the test backend, as many as a deployment binds.
pub const SHARDS: u32 = 8;

/// The placement a fixture written by hand gets: two shards, by the parity of the id.
pub fn by_parity(id: u64) -> u32 {
    u32::from(id % 2 == 1)
}

/// Tells apart the shared databases of backends made at once by tests running together.
static BACKENDS: AtomicU64 = AtomicU64::new(0);

/// A test backend's object shards, each a connection of its own, and the placement of an object
/// the index names no shard for.
pub struct Shards {
    shards: Vec<Connection>,
    place: Shared<fn(u64) -> u32>,
}

impl Shards {
    /// Shards with the release schema, attached to `catalogue` as `shard0` … `shard7`.
    pub fn new(catalogue: &Connection) -> Self {
        let backend = BACKENDS.fetch_add(1, Ordering::Relaxed);
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let shards = (0..SHARDS)
            .map(|i| {
                let uri = format!("file:univ-api-{backend}-shard-{i}?mode=memory&cache=shared");
                let shard = Connection::open_with_flags(&uri, flags).unwrap();
                shard.execute_batch(SCHEMA).unwrap();
                catalogue
                    .execute(&format!("ATTACH DATABASE '{uri}' AS shard{i}"), [])
                    .unwrap();
                shard
            })
            .collect();
        Self {
            shards,
            place: Shared::new(by_parity),
        }
    }

    /// Shard `index`, none past the last.
    pub fn get(&self, index: u32) -> Option<&Connection> {
        self.shards.get(usize::try_from(index).ok()?)
    }

    /// Places each object written from now on that the index names no shard for by `place`.
    pub fn place_by(&self, place: fn(u64) -> u32) {
        self.place.set(place);
    }

    /// Moves the rows of the shard tables `catalogue` holds to their objects' shards, giving a
    /// shard to each object the index names none for, then writes the index of the shards.
    pub fn distribute(&self, catalogue: &Connection) {
        let unmoved = SHARD_TABLES
            .map(|t| format!("EXISTS (SELECT 1 FROM main.{t})"))
            .join(" OR ");
        let moving: bool = catalogue
            .query_row(&format!("SELECT {unmoved}"), [], |r| r.get(0))
            .unwrap();
        if !moving {
            return;
        }
        let unplaced: Vec<(i64, u64, Option<i64>)> = catalogue
            .prepare(
                "SELECT release, id, retired FROM main.objects o WHERE NOT EXISTS \
                 (SELECT 1 FROM object_shards s WHERE s.id = o.id AND s.release <= o.release \
                 AND (s.retired IS NULL OR s.retired > o.release))",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let place = self.place.get();
        for (release, id, retired) in unplaced {
            catalogue
                .execute(
                    "INSERT INTO object_shards VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![release, id, place(id), retired],
                )
                .unwrap();
        }
        for table in SHARD_TABLES {
            let id = if table == "objects" {
                "id"
            } else {
                "object_id"
            };
            // An object stays in one shard: a later release's rows of it go where its first went.
            let owner = format!("SELECT 1 FROM object_shards s WHERE s.id = t.{id}");
            for i in 0..SHARDS {
                let sql = format!(
                    "INSERT INTO shard{i}.{table} SELECT * FROM main.{table} t \
                     WHERE EXISTS ({owner} AND s.shard = ?1)"
                );
                catalogue.execute(&sql, [i]).unwrap();
            }
            let sql = format!("DELETE FROM main.{table} AS t WHERE EXISTS ({owner})");
            catalogue.execute(&sql, []).unwrap();
        }
        self.index(catalogue);
    }

    /// Writes each shard's objects and the range of its cells' codes of each domain.
    fn index(&self, catalogue: &Connection) {
        catalogue
            .execute_batch(
                "DELETE FROM release_shards; \
                 INSERT INTO release_shards \
                 SELECT MIN(release), shard, COUNT(*), 0, NULL FROM object_shards GROUP BY shard;",
            )
            .unwrap();
        for (i, shard) in (0_u32..).zip(&self.shards) {
            // Each domain's run: the first release with a cell in it, and the range of codes.
            let mut ranges: BTreeMap<i64, (i64, u64, u64)> = BTreeMap::new();
            let mut cells = shard
                .prepare("SELECT release, domain, ix, iy, iz FROM object_cells")
                .unwrap();
            let rows = cells
                .query_map([], |r| {
                    let cell = Cell {
                        ix: r.get(2)?,
                        iy: r.get(3)?,
                        iz: r.get(4)?,
                    };
                    Ok((r.get(0)?, r.get(1)?, cell))
                })
                .unwrap();
            for row in rows {
                let (release, domain, cell) = row.unwrap();
                let code = morton(cell).unwrap();
                let run = ranges.entry(domain).or_insert((release, code, code));
                *run = (run.0.min(release), run.1.min(code), run.2.max(code));
            }
            for (domain, (release, first, last)) in ranges {
                // One run for each shard and domain: it replaces any the fixture gave.
                catalogue
                    .execute(
                        "DELETE FROM shard_cells WHERE shard = ?1 AND domain = ?2",
                        rusqlite::params![i, domain],
                    )
                    .unwrap();
                catalogue
                    .execute(
                        "INSERT INTO shard_cells VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                        rusqlite::params![release, i, domain, first, last],
                    )
                    .unwrap();
            }
        }
    }
}
