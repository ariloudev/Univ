//! The test backend's object shards: in-memory SQLite databases beside the catalogue, each with
//! the release schema, shared with the catalogue's connection by name so that it can move rows
//! into them.
//!
//! A fixture writes every row to the catalogue. Its rows of the shard tables are then moved to
//! the shard the catalogue's `object_shards` names for their object, as a published release has
//! them. An object it names no shard for is given shard `id % 2`, with its cells added to its
//! release's `shard_cells` and its count to `release_shards`: a fixture written by hand spans
//! two shards, and every read is tested across them.

use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OpenFlags};
use univ_model::release::shards::{SHARD_TABLES, morton};
use univ_model::release::{Cell, SCHEMA};

/// The shards of the test backend, as many as a deployment binds.
pub const SHARDS: u32 = 8;

/// The shards a fixture written by hand spreads its objects over.
const SPREAD: i64 = 2;

/// Tells apart the shared databases of backends made at once by tests running together.
static BACKENDS: AtomicU64 = AtomicU64::new(0);

/// A test backend's object shards, each a connection of its own.
pub struct Shards(Vec<Connection>);

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
        Self(shards)
    }

    /// Shard `index`, none past the last.
    pub fn get(&self, index: u32) -> Option<&Connection> {
        self.0.get(usize::try_from(index).ok()?)
    }

    /// Moves the rows of the shard tables `catalogue` holds to their objects' shards, first
    /// giving a shard to each object the index names none for.
    pub fn distribute(catalogue: &Connection) {
        catalogue
            .execute_batch(&format!(
                "CREATE TEMP TABLE IF NOT EXISTS spread (release INTEGER, id INTEGER);
                 DELETE FROM spread;
                 INSERT INTO spread SELECT release, id FROM main.objects o WHERE NOT EXISTS
                   (SELECT 1 FROM object_shards s WHERE s.release = o.release AND s.id = o.id);
                 INSERT INTO object_shards SELECT release, id, id % {SPREAD} FROM spread;
                 INSERT INTO release_shards SELECT release, id % {SPREAD}, COUNT(*), 0
                   FROM spread WHERE true GROUP BY 1, 2
                   ON CONFLICT (release, shard) DO UPDATE SET objects = objects + excluded.objects;"
            ))
            .unwrap();
        spread_cells(catalogue);
        for table in SHARD_TABLES {
            let id = if table == "objects" {
                "id"
            } else {
                "object_id"
            };
            let owned = format!(
                "EXISTS (SELECT 1 FROM object_shards s WHERE s.release = t.release \
                 AND s.id = t.{id} AND s.shard = ?1)"
            );
            for i in 0..SHARDS {
                catalogue
                    .execute(
                        &format!(
                            "INSERT INTO shard{i}.{table} SELECT * FROM main.{table} t WHERE {owned}"
                        ),
                        [i],
                    )
                    .unwrap();
                catalogue
                    .execute(&format!("DELETE FROM main.{table} AS t WHERE {owned}"), [i])
                    .unwrap();
            }
        }
    }
}

/// Widens each `shard_cells` range of the objects just given a shard to hold their cells.
fn spread_cells(catalogue: &Connection) {
    let mut cells = catalogue
        .prepare(
            "SELECT c.release, c.object_id % ?1, c.domain, c.ix, c.iy, c.iz FROM main.object_cells c \
             JOIN spread s ON s.release = c.release AND s.id = c.object_id",
        )
        .unwrap();
    let rows: Vec<(i64, i64, i64, Cell)> = cells
        .query_map([SPREAD], |r| {
            let cell = Cell {
                ix: r.get(3)?,
                iy: r.get(4)?,
                iz: r.get(5)?,
            };
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, cell))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut widen = catalogue
        .prepare(
            "INSERT INTO shard_cells VALUES (?1, ?2, ?3, ?4, ?4) ON CONFLICT (release, shard, \
             domain) DO UPDATE SET first = min(first, excluded.first), \
             last = max(last, excluded.last)",
        )
        .unwrap();
    for (release, shard, domain, cell) in rows {
        let code = i64::try_from(morton(cell).unwrap()).unwrap();
        widen
            .execute(rusqlite::params![release, shard, domain, code])
            .unwrap();
    }
}
