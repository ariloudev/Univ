//! A release's rows read from the local D1 that `wrangler dev` keeps (`--store`), rather than
//! from the builds of its chain: what checks a release whose parent's build was pruned.
//!
//! Miniflare names each database's file by a hash of its id, so each is told by what it holds,
//! as `univ-ingest reindex` tells them: every database is given the whole schema, so the
//! catalogue is the one whose `latest` points to a release, and an object shard is a database
//! that holds an object of the release that `object_shards` places in a shard. Each shard
//! the release has must be found, and none twice.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use univ_model::release::{LOCAL_D1_FILES, SCHEMA, Scope, SqlValue, TABLES, valid_in};

fn one(db: &Connection, sql: &str) -> Result<Option<i64>, String> {
    db.query_row(sql, [], |r| r.get::<_, Option<i64>>(0))
        .optional()
        .map(Option::flatten)
        .map_err(|e| format!("{sql}: {e}"))
}

fn holds(db: &Connection, schema: &str, name: &str) -> Result<bool, String> {
    let sql = format!(
        "SELECT COUNT(*) FROM {schema}.sqlite_master WHERE type = 'table' AND name = '{name}'"
    );
    Ok(one(db, &sql)?.unwrap_or(0) > 0)
}

/// The files of the local D1 in `persist_to` that hold release `n`: the catalogue's first.
fn files_of(persist_to: &Path, n: u32) -> Result<Vec<PathBuf>, String> {
    let dir = persist_to.join(LOCAL_D1_FILES);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sqlite"))
        .collect();
    files.sort();
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut catalogue = None;
    let mut others = Vec::new();
    for path in files {
        let db = Connection::open_with_flags(&path, flags)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !holds(&db, "main", "objects")? {
            continue;
        }
        if one(&db, "SELECT release FROM latest")?.is_some() {
            if catalogue.is_some() {
                return Err(format!(
                    "more than one database in {} has latest",
                    dir.display()
                ));
            }
            catalogue = Some((path, db));
        } else {
            others.push((path, db));
        }
    }
    let (path, db) =
        catalogue.ok_or_else(|| format!("no database in {} has latest", dir.display()))?;
    let listed = format!("SELECT COUNT(*) FROM releases WHERE number = {n} AND pruned_at IS NULL");
    if one(&db, &listed)? != Some(1) {
        return Err(format!("the local D1 holds no release {n} left unpruned"));
    }
    let valid = valid_in("", n);
    let mut shards = BTreeMap::new();
    for (file, shard) in others {
        let Some(id) = one(
            &shard,
            &format!("SELECT id FROM objects WHERE {valid} LIMIT 1"),
        )?
        else {
            continue;
        };
        let sql = format!("SELECT shard FROM object_shards WHERE id = {id} AND {valid}");
        let at = one(&db, &sql)?
            .ok_or_else(|| format!("{} holds object {id}, which no shard holds", file.display()))?;
        if shards.insert(at, file).is_some() {
            return Err(format!("two databases hold shard {at} of release {n}"));
        }
    }
    let count = format!("SELECT COUNT(DISTINCT shard) FROM release_shards WHERE {valid}");
    let wanted = one(&db, &count)?.unwrap_or(0);
    if usize::try_from(wanted).ok() != Some(shards.len()) {
        return Err(format!(
            "release {n} has {wanted} object shards, and {} databases hold its objects",
            shards.len()
        ));
    }
    Ok(std::iter::once(path).chain(shards.into_values()).collect())
}

/// Loads release `n` from the local D1 in `persist_to` into `db` as one release written whole:
/// the rows of each table as the release reads them, those of tables whose rows releases share
/// each as its own, as [`super::load::load`] leaves them. Gives the databases read.
///
/// # Errors
///
/// Fails when the local D1's databases cannot be told apart or read, or do not hold release
/// `n` left unpruned.
pub fn load(db: &Connection, persist_to: &Path, n: u32) -> Result<usize, String> {
    let files = files_of(persist_to, n)?;
    db.execute_batch(SCHEMA)
        .map_err(|e| format!("cannot create the schema: {e}"))?;
    // No transaction: SQLite attaches no database within one, and the scratch database keeps
    // no journal.
    for file in &files {
        let uri = SqlValue::from(format!("file:{}?mode=ro", file.display())).literal();
        db.execute_batch(&format!("ATTACH DATABASE {uri} AS store"))
            .map_err(|e| format!("{}: {e}", file.display()))?;
        for table in &TABLES {
            if !holds(db, "store", table.name)? {
                continue;
            }
            // A shared row is taken as the release's own: its release `n`, `retired` left NULL.
            let columns = table.columns.join(", ");
            let values = table
                .columns
                .iter()
                .map(|c| {
                    if table.scope == Scope::PerRelease && *c == "release" {
                        n.to_string()
                    } else {
                        (*c).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            db.execute_batch(&format!(
                "INSERT OR IGNORE INTO main.{name} ({columns}) SELECT {values} FROM \
                 store.{name} WHERE {filter};",
                name = table.name,
                filter = table.scope.filter(n)
            ))
            .map_err(|e| format!("{} of {}: {e}", table.name, file.display()))?;
        }
        db.execute_batch("DETACH DATABASE store")
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(files.len())
}
