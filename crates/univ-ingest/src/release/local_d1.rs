//! The local D1 that `wrangler dev` keeps, read directly from its `SQLite` files for a reindex:
//! Miniflare names each file by a hash of its database's id, so each is told by what it holds.
//! Every database is given the whole schema, so the catalogue is the one whose `latest` points to
//! a release, and each object shard is the one `object_shards` names for an object it holds of
//! that release. A shard that holds nothing of it is left out, as it holds no row to index, and so
//! is a database bound but never loaded, whatever schema it was given.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use univ_model::release::shards::Database;
use univ_model::release::{LOCAL_D1_FILES, SCHEMA_VERSION, valid_in};

use super::reindex::{RowSource, holds};

/// The databases of a local D1 by what each holds, with the release `latest` points to.
pub struct LocalD1 {
    databases: BTreeMap<Database, Connection>,
    /// The release `latest` points to.
    pub latest: u32,
}

fn err(path: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", path.display())
}

/// A file's name, which is all that tells Miniflare's files apart.
fn shown(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// `path`, opened read only. A database whose changes are still in its write-ahead log is read
/// with them, beside a running `wrangler dev`.
fn open(path: &Path) -> Result<Connection, String> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(path, flags).map_err(|e| err(path, e))
}

fn one<T: rusqlite::types::FromSql>(db: &Connection, sql: &str) -> Result<Option<T>, String> {
    db.query_row(sql, [], |r| r.get::<_, T>(0))
        .optional()
        .map_err(|e| format!("{sql}: {e}"))
}

impl LocalD1 {
    /// The local D1 kept in Wrangler's state directory `persist_to`.
    ///
    /// # Errors
    ///
    /// Fails when no database or more than one is the catalogue, when it is older than schema
    /// 6, when a database holds objects of an older schema, when two files hold one shard, or
    /// when the shards found are not those the release has.
    pub fn open(persist_to: &Path) -> Result<Self, String> {
        let dir = persist_to.join(LOCAL_D1_FILES);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| err(&dir, e))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "sqlite"))
            .collect();
        files.sort();
        let mut catalogue: Option<(PathBuf, Connection, i64)> = None;
        let mut others = Vec::new();
        for path in files {
            let db = open(&path)?;
            if !holds(&db, "objects")? {
                continue;
            }
            match one::<i64>(&db, "SELECT release FROM latest")? {
                Some(latest) => {
                    if let Some((other, ..)) = &catalogue {
                        return Err(format!(
                            "both {} and {} have latest point to a release",
                            shown(other),
                            shown(&path)
                        ));
                    }
                    catalogue = Some((path, db, latest));
                }
                None => others.push((path, db)),
            }
        }
        let (path, db, latest) =
            catalogue.ok_or_else(|| format!("no database in {} has latest", dir.display()))?;
        let latest = u32::try_from(latest).map_err(|e| err(&path, e))?;
        // Its own queries read `retired`, which schema 6 added.
        let version = one::<i64>(&db, "SELECT MAX(version) FROM schema_version")?.unwrap_or(0);
        if version < i64::from(SCHEMA_VERSION) {
            return Err(format!(
                "the local D1 is of schema {version}, older than {SCHEMA_VERSION}: run \
                 `univ-ingest migrate` first"
            ));
        }
        let valid = valid_in("", latest);
        let mut databases = BTreeMap::new();
        for (file, shard) in others {
            // A database bound but never loaded, at whatever schema it was given.
            if one::<i64>(&shard, "SELECT id FROM objects LIMIT 1")?.is_none() {
                continue;
            }
            let shared = "SELECT COUNT(*) FROM pragma_table_info('objects') WHERE name = 'retired'";
            if one::<i64>(&shard, shared)? != Some(1) {
                return Err(format!(
                    "{} holds objects of a schema older than {SCHEMA_VERSION}: run `univ-ingest \
                     migrate` first",
                    shown(&file)
                ));
            }
            let sql = format!("SELECT id FROM objects WHERE {valid} LIMIT 1");
            let Some(id) = one::<i64>(&shard, &sql)? else {
                continue;
            };
            let sql = format!("SELECT shard FROM object_shards WHERE id = {id} AND {valid}");
            let n = one::<i64>(&db, &sql)?.ok_or_else(|| {
                format!("{} holds object {id}, which no shard holds", shown(&file))
            })?;
            let n = u32::try_from(n).map_err(|e| err(&file, e))?;
            if databases.insert(Database::Shard(n), shard).is_some() {
                return Err(format!(
                    "{} holds shard {n}, which another file holds",
                    shown(&file)
                ));
            }
        }
        let sql = format!("SELECT COUNT(DISTINCT shard) FROM release_shards WHERE {valid}");
        let shards = one::<i64>(&db, &sql)?.unwrap_or(0);
        if usize::try_from(shards).ok() != Some(databases.len()) {
            return Err(format!(
                "release {latest} has {shards} object shards, and {} files hold its objects",
                databases.len()
            ));
        }
        databases.insert(Database::Catalogue, db);
        Ok(Self { databases, latest })
    }
}

impl RowSource for LocalD1 {
    fn databases(&self) -> Vec<Database> {
        self.databases.keys().copied().collect()
    }

    fn db(&self, database: Database) -> Result<&Connection, String> {
        self.databases
            .get(&database)
            .ok_or_else(|| format!("the local D1 holds no {database}"))
    }
}
