//! The indexes the checks search the tables per object and per catalogue entry by, beside the
//! release schema's own, and the statistics their plans are made with (the scan guard,
//! [`super::plan`]).
//!
//! The indexes are made in the scratch database alone: D1 keeps only the indexes Univ Server
//! reads by, and these would cost every object shard space for the checks' sake. The statistics
//! are those of a full release, `rules/science-statistics.yaml`, loaded into every scratch
//! database, the fixture's too: without them SQLite takes any index over a scan, and searches
//! by a column most rows share (`placed = 1`) through millions of lookups; with them it plans as
//! for a release of tens of millions of objects, wherever the checks run.

use std::path::Path;

use rusqlite::Connection;
use serde::Deserialize;

/// Each check index: its name, and its table, columns and, for a partial index, the rows it
/// keeps, as `CREATE INDEX` gives them.
const CHECK_INDEXES: &[(&str, &str)] = &[
    ("check_entries_by_source", "entries (source, release)"),
    (
        "check_measurements_by_quantity",
        "measurements (quantity, release)",
    ),
    (
        "check_objects_by_domain",
        "objects (scale_domain, placed, distance_mpc)",
    ),
    (
        "check_objects_by_distance",
        "objects (placed, distance_mpc)",
    ),
    ("check_objects_by_kind", "objects (kind, placed)"),
    (
        "check_objects_by_preferred_quantity",
        "objects (preferred_quantity, placed)",
    ),
    // Partial: only the placed objects with no distance or position, which
    // no-negative-or-missing-distances lists. Made in one pass over objects.
    (
        "check_objects_misplaced",
        "objects (id) WHERE placed = 1 AND (distance_mpc IS NULL OR NOT distance_mpc > 0 \
         OR x IS NULL OR y IS NULL OR z IS NULL)",
    ),
];

/// The statistics of a full release's tables and indexes, as `ANALYZE` wrote them.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Statistics {
    /// The release they were read from.
    #[expect(dead_code, reason = "read by people, not by the loader")]
    release: u32,
    rows: Vec<StatRow>,
}

/// One row of `sqlite_stat1`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatRow {
    table: String,
    index: Option<String>,
    stat: String,
}

/// Makes [`CHECK_INDEXES`] in `db`, again where one of the same name has another definition,
/// and loads the statistics of `statistics` for the planner.
///
/// # Errors
///
/// Fails when an index cannot be made or the statistics cannot be read or loaded.
pub fn index(db: &Connection, statistics: &Path) -> Result<(), String> {
    // CREATE INDEX sorts its rows: let the sorter use worker threads.
    db.execute_batch("PRAGMA threads = 8")
        .map_err(|e| format!("cannot set up the sorter: {e}"))?;
    for (name, definition) in CHECK_INDEXES {
        let sql = format!("CREATE INDEX {name} ON {definition}");
        let held: Option<String> = db
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'index' AND name = ?1",
                [name],
                |r| r.get(0),
            )
            .ok();
        if held.as_deref() == Some(sql.as_str()) {
            continue;
        }
        if held.is_some() {
            db.execute_batch(&format!("DROP INDEX {name}"))
                .map_err(|e| format!("cannot drop {name}: {e}"))?;
        }
        db.execute_batch(&sql)
            .map_err(|e| format!("cannot make {name} for the checks: {e}"))?;
    }
    // A check index the list no longer names, left by an earlier run on a kept `--db`.
    let mut held = db
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'index' AND name GLOB 'check_*'")
        .map_err(|e| e.to_string())?;
    let held: Vec<String> = held
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    for name in held
        .iter()
        .filter(|h| !CHECK_INDEXES.iter().any(|(n, _)| n == h))
    {
        db.execute_batch(&format!("DROP INDEX {name}"))
            .map_err(|e| format!("cannot drop {name}: {e}"))?;
    }
    load_statistics(db, statistics)
}

/// Replaces `db`'s statistics by those of `path`, and has the planner read them.
fn load_statistics(db: &Connection, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let statistics: Statistics =
        serde_norway::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let failed = |e: rusqlite::Error| format!("cannot load {}: {e}", path.display());
    // ANALYZE of the schema table makes sqlite_stat1 where it is missing, and reads it again.
    db.execute_batch("ANALYZE sqlite_schema; DELETE FROM sqlite_stat1;")
        .map_err(failed)?;
    let mut insert = db
        .prepare("INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?1, ?2, ?3)")
        .map_err(failed)?;
    for row in &statistics.rows {
        insert
            .execute((&row.table, &row.index, &row.stat))
            .map_err(failed)?;
    }
    db.execute_batch("ANALYZE sqlite_schema").map_err(failed)
}
