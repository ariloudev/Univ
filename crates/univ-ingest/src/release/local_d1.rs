//! The local D1 that `wrangler dev` keeps, read directly from its SQLite files for a reindex:
//! Miniflare names each file by a hash of its database's id, so each is told by what it holds.
//! Every database is given the whole schema, so the catalogue is the one whose `latest` points to
//! a release, and each object shard is the one `object_shards` names for an object it holds of
//! that release. A shard that holds nothing of it is left out, as it holds no row to index.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use univ_model::release::shards::Database;
use univ_model::release::{SCHEMA_VERSION, SqlValue, Table, valid_in};

use super::reindex::RowSource;

/// Where Wrangler keeps the local D1's files, within its state directory `--persist-to`.
pub const D1_FILES: &str = "v3/d1/miniflare-D1DatabaseObject";

/// The databases of a local D1 by what each holds, with the release `latest` points to.
pub struct LocalD1 {
    databases: BTreeMap<Database, (PathBuf, Connection)>,
    /// The release `latest` points to.
    pub latest: u32,
    /// The catalogue's schema version.
    pub version: i64,
}

fn err(path: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", path.display())
}

/// `path`, opened read only. A database whose changes are still in its write-ahead log is read
/// with them, beside a running `wrangler dev`.
fn open(path: &Path) -> Result<Connection, String> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(path, flags).map_err(|e| err(path, e))
}

fn holds(db: &Connection, name: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
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
    /// 6, when two files hold one shard, or when the shards found are not those the release has.
    pub fn open(persist_to: &Path) -> Result<Self, String> {
        let dir = persist_to.join(D1_FILES);
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
            let sql = format!("SELECT id FROM objects WHERE {valid} LIMIT 1");
            let Some(id) = one::<i64>(&shard, &sql)? else {
                continue;
            };
            let sql = format!("SELECT shard FROM object_shards WHERE id = {id} AND {valid}");
            let n = one::<i64>(&db, &sql)?.ok_or_else(|| {
                format!("{} holds object {id}, which no shard holds", shown(&file))
            })?;
            let n = u32::try_from(n).map_err(|e| err(&file, e))?;
            if let Some((before, _)) = databases.insert(Database::Shard(n), (file.clone(), shard)) {
                return Err(format!(
                    "both {} and {} hold shard {n}",
                    shown(&before),
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
        databases.insert(Database::Catalogue, (path, db));
        Ok(Self {
            databases,
            latest,
            version,
        })
    }

    /// The file holding each database.
    pub fn files(&self) -> impl Iterator<Item = (Database, &Path)> {
        self.databases.iter().map(|(d, (p, _))| (*d, p.as_path()))
    }

    fn db(&self, database: Database) -> Result<&Connection, String> {
        self.databases
            .get(&database)
            .map(|(_, db)| db)
            .ok_or_else(|| format!("the local D1 holds no {database}"))
    }
}

/// A file's name, which is all that tells Miniflare's files apart.
fn shown(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl RowSource for LocalD1 {
    fn databases(&mut self) -> Result<Vec<Database>, String> {
        Ok(self.databases.keys().copied().collect())
    }

    fn holds(&mut self, database: Database, name: &str) -> Result<bool, String> {
        holds(self.db(database)?, name)
    }

    fn rows(
        &mut self,
        database: Database,
        table: &Table,
        filter: &str,
        each: &mut dyn FnMut(&[SqlValue]) -> Result<(), String>,
    ) -> Result<(), String> {
        let db = self.db(database)?;
        let sql = format!(
            "SELECT {} FROM {} WHERE {filter}",
            table.columns.join(", "),
            table.name
        );
        let mut q = db.prepare(&sql).map_err(|e| format!("{sql}: {e}"))?;
        let mut rows = q.query([]).map_err(|e| format!("{sql}: {e}"))?;
        let mut values = Vec::with_capacity(table.columns.len());
        while let Some(row) = rows.next().map_err(|e| format!("{sql}: {e}"))? {
            values.clear();
            for i in 0..table.columns.len() {
                values.push(match row.get::<_, Value>(i).map_err(|e| e.to_string())? {
                    Value::Null | Value::Blob(_) => SqlValue::Null,
                    Value::Integer(n) => SqlValue::Integer(n),
                    Value::Real(x) => SqlValue::Real(x),
                    Value::Text(t) => SqlValue::Text(t),
                });
            }
            each(&values)?;
        }
        Ok(())
    }
}
