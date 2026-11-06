//! A release's D1 rows written as SQL files ready for `wrangler d1 execute --file`, one row at a
//! time, with the estimate of the storage they take, each table's fingerprint and the checks of
//! its rows: nothing keeps a table's rows.
//!
//! A release is written to several databases (`univ_model::release::shards`): the catalogue
//! database and each object shard. An object's rows of the shard tables go to the shard chosen
//! for it ([`D1Writer::set_shard`]), and every other row to the catalogue. Each database's
//! files are under `d1/{dir}/`, `dir` being `catalogue` or `shard-{i:02}`: `0000-schema.sql`,
//! the whole schema and its version, then `{nnnn}-{table}-{part}.sql` for each table that has
//! rows there, `nnnn` its place in `univ_model::release::TABLES`, each part at most
//! [`MAX_FILE_BYTES`]. Executed in path order they load the database; each holds
//! `INSERT OR IGNORE` statements only (D1 runs a file without explicit transactions), so a file
//! executed twice changes nothing. The writer checks that each table's primary keys are unique
//! and every number finite, so `INSERT OR IGNORE` can only skip a row written before,
//! identical, and no NULL stands for a number.

mod compare;
mod index;
mod indexer;
mod parts;
mod state;

pub use compare::Before;
pub use index::{ROWS, fresh as fresh_index};
pub use indexer::Indexer;
pub use parts::MAX_FILE_BYTES;

use sha2::{Digest, Sha256};
use univ_model::release::fingerprint::canonical_row;
use univ_model::release::manifest::{D1_CAP_BYTES, D1Estimate, TableEstimate};
use univ_model::release::shards::Database;
use univ_model::release::{
    RETIRED, Row, SCHEMA, SCHEMA_VERSION, SqlValue, Statements, TABLES, Table, keys,
};

use index::{Indexed, hash_at};
use parts::{Kind, Place};
use state::{TableState, key_of, place_of};

use super::check::{D1_FINITE, D1_KEYS, Failed};

/// Where the writer puts each file it completes: its path in the release directory and bytes.
pub trait PartSink {
    /// Takes one file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be kept.
    fn put(&mut self, path: String, bytes: &[u8]) -> Result<(), String>;
}

/// Files kept in memory, in the order written.
#[derive(Debug, Default)]
pub struct InMemory {
    pub files: Vec<(String, Vec<u8>)>,
}

impl PartSink for InMemory {
    fn put(&mut self, path: String, bytes: &[u8]) -> Result<(), String> {
        self.files.push((path, bytes.to_vec()));
        Ok(())
    }
}

/// The SHA-256, in hex, of `rows` in their canonical form, in the order given.
#[must_use]
pub fn content_sha256(rows: &[Vec<SqlValue>]) -> String {
    let mut bytes = Vec::new();
    for row in rows {
        canonical_row(row, &mut bytes);
    }
    crate::fetch::hex(&Sha256::digest(&bytes))
}

/// The D1 files of a release, written as its rows come, or, without a sink, only the estimate
/// of the bytes they would take.
pub struct D1Writer<'s> {
    sink: Option<&'s mut dyn PartSink>,
    /// The tables of each database: the catalogue's, then each shard's ([`slot`]).
    databases: Vec<Vec<TableState>>,
    /// The estimated bytes of each database's rows so far.
    bytes: Vec<u64>,
    /// The shard of the object whose rows are being written.
    shard: Option<u32>,
    /// The release whose rows are compared with the release before's, where they are.
    comparing: Option<u32>,
    /// A row's key as SQL literals, for the row index: one buffer for every row.
    key_text: String,
}

/// What the writer made of a release's rows once every file is written.
#[derive(Debug)]
pub struct D1Written {
    /// The catalogue database's rows.
    pub estimate: D1Estimate,
    /// Each object shard's rows, in order.
    pub shards: Vec<D1Estimate>,
    /// The checks of the rows that failed.
    pub failed: Vec<Failed>,
}

/// The database at `index` among the writer's: the catalogue, then the shards.
fn database(index: usize) -> Database {
    match index {
        0 => Database::Catalogue,
        i => Database::Shard(u32::try_from(i - 1).unwrap_or(u32::MAX)),
    }
}

/// The place of `database` among the writer's, [`database`]'s inverse.
fn slot(database: Database) -> usize {
    match database {
        Database::Catalogue => 0,
        Database::Shard(i) => i as usize + 1,
    }
}

impl<'s> D1Writer<'s> {
    /// A writer into `sink` of the catalogue and `shards` object shards, which takes each
    /// database's schema file first.
    ///
    /// # Errors
    ///
    /// Fails when the sink cannot take them.
    pub fn new(sink: &'s mut dyn PartSink, shards: u32) -> Result<Self, String> {
        let schema = format!(
            "{SCHEMA}\nINSERT OR IGNORE INTO schema_version (version) VALUES ({SCHEMA_VERSION});\n"
        );
        let mut writer = Self::measuring(shards);
        for i in 0..writer.databases.len() {
            let path = format!("{}/{}/0000-schema.sql", keys::D1_DIR, database(i).dir());
            sink.put(path, schema.as_bytes())?;
        }
        writer.sink = Some(sink);
        Ok(writer)
    }

    /// A writer that writes nothing and only adds up the estimated bytes of the rows it is
    /// given, as [`D1Writer::estimated_bytes_in`] gives them.
    #[must_use]
    pub fn measuring(shards: u32) -> Self {
        let databases: Vec<Vec<TableState>> = (0..=shards)
            .map(|_| TABLES.iter().copied().map(TableState::new).collect())
            .collect();
        Self {
            sink: None,
            bytes: vec![0; databases.len()],
            databases,
            shard: None,
            comparing: None,
            key_text: String::new(),
        }
    }

    /// Sends the rows of the shard tables that follow to object shard `shard`, until another
    /// is set or [`D1Writer::clear_shard`] is called.
    pub fn set_shard(&mut self, shard: u32) {
        self.shard = Some(shard);
    }

    /// Sends no more rows of the shard tables anywhere: one that follows is an error.
    pub fn clear_shard(&mut self) {
        self.shard = None;
    }

    /// Writes `row`.
    ///
    /// # Errors
    ///
    /// Fails when its table is not of the schema, it is a row of a shard table and no shard is
    /// set or the shard is not one of the writer's, or a file cannot be put.
    pub fn add<R: Row>(&mut self, row: &R) -> Result<(), String> {
        let place = const { place_of(R::TABLE.name) };
        self.add_at(place, &R::TABLE, &row.values())
    }

    /// Writes each of `rows`.
    ///
    /// # Errors
    ///
    /// As [`D1Writer::add`].
    pub fn add_all<R: Row>(&mut self, rows: &[R]) -> Result<(), String> {
        rows.iter().try_for_each(|r| self.add(r))
    }

    /// Writes a row of `table`, its values in the order of its columns.
    ///
    /// # Errors
    ///
    /// As [`D1Writer::add`].
    pub fn add_values(&mut self, table: &Table, values: &[SqlValue]) -> Result<(), String> {
        self.add_at(place_of(table.name), table, values)
    }

    /// The place among the writer's databases of the one a row of `table` goes to.
    fn route(&self, table: &Table) -> Result<usize, String> {
        if !table.in_shard() {
            return Ok(0);
        }
        let shard = self
            .shard
            .ok_or_else(|| format!("a row of {} has no object shard set", table.name))?;
        let index = slot(Database::Shard(shard));
        if index < self.databases.len() {
            Ok(index)
        } else {
            Err(format!(
                "a row of {} is sent to object shard {shard}, which the release does not have",
                table.name
            ))
        }
    }

    /// Writes a row of `table`, the table at `place` in `TABLES`.
    fn add_at(&mut self, place: usize, table: &Table, values: &[SqlValue]) -> Result<(), String> {
        let db = self.route(table)?;
        let state = self.databases[db]
            .get_mut(place)
            .ok_or_else(|| format!("{} is not a table of the schema", table.name))?;
        let bytes = table.estimated_bytes(values);
        state.rows += 1;
        state.bytes += bytes;
        self.bytes[db] += bytes;
        let Some(sink) = self.sink.as_deref_mut() else {
            return Ok(());
        };
        let key = hash_at(&state.key, values);
        // A row the release before holds unchanged: kept with the release that added it.
        let mut kept = None;
        if let (Some(number), Some(next)) = (self.comparing, state.next.as_mut()) {
            let content = hash_at(&state.content_at, values);
            kept = state
                .previous
                .as_mut()
                .and_then(|p| p.compare(key, content));
            index::key_text(&state.key, values, &mut self.key_text);
            let release = kept.unwrap_or(number);
            next.record(
                Indexed {
                    key,
                    content,
                    release,
                },
                &self.key_text,
            )?;
        }
        let kept_release = kept.map(|r| SqlValue::Integer(i64::from(r)));
        match (&kept_release, state.release_at) {
            (Some(r), Some(at)) => state.fingerprint.add_replacing(values, at, r),
            _ => state.fingerprint.add(values),
        }
        if let Some(content) = &mut state.content {
            let mut row = values.to_vec();
            if let (Some(r), Some(at)) = (kept_release, state.release_at) {
                row[at] = r;
            }
            content.push(row);
        }
        if state.not_finite.is_none()
            && let Some((x, column)) =
                values
                    .iter()
                    .zip(table.columns)
                    .find_map(|(v, column)| match v {
                        SqlValue::Real(x) if !x.is_finite() => Some((x, column)),
                        _ => None,
                    })
        {
            state.not_finite = Some(format!(
                "table {}, row {}, column {column}: {x} is not a finite number",
                table.name,
                key_of(table, values)
            ));
        }
        if !state.keys.insert(key) {
            state.repeated += 1;
            if state.first_repeated.is_none() {
                state.first_repeated = Some(key_of(table, values));
            }
        }
        if kept.is_some() {
            return Ok(());
        }
        state.added += 1;
        if let Some(statement) = state.statements.push(values) {
            let to = Place {
                database: database(db),
                at: place,
                table: table.name,
                kind: Kind::Rows,
            };
            state.parts.push(sink, to, &statement)?;
        }
        Ok(())
    }

    /// Forgets the keys seen of `tables`, whose keys hold the object id, once an object's rows
    /// are all written: their keys are unique if unique within each object, the object's id
    /// being unique among the objects.
    ///
    /// Only the catalogue and the shard set are touched; a writer that only measures keeps no
    /// keys.
    pub fn forget_keys(&mut self, tables: &[&str]) {
        if self.sink.is_none() {
            return;
        }
        let shard = self.shard.map_or(0, |s| slot(Database::Shard(s)));
        for db in [0, shard] {
            for state in &mut self.databases[db] {
                if tables.contains(&state.table.name) {
                    state.keys.clear();
                }
            }
        }
    }

    /// The estimated D1 bytes of the rows written so far to `database`.
    #[must_use]
    pub fn estimated_bytes_in(&self, database: Database) -> u64 {
        self.bytes.get(slot(database)).copied().unwrap_or(0)
    }

    /// The rows written so far to the table `name`, in every database.
    #[must_use]
    pub fn rows(&self, name: &str) -> u64 {
        self.databases
            .iter()
            .flatten()
            .filter(|t| t.table.name == name)
            .map(|t| t.rows)
            .sum()
    }

    /// Puts each table's last part in each database, and gives their estimates and the failed
    /// checks.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be put.
    pub fn finish(mut self) -> Result<D1Written, String> {
        self.retire()?;
        let mut estimates = Vec::with_capacity(self.databases.len());
        let mut failed = Vec::new();
        for (db, tables) in std::mem::take(&mut self.databases).into_iter().enumerate() {
            let mut estimate = Vec::with_capacity(tables.len());
            for (place, state) in tables.into_iter().enumerate() {
                estimate.push(self.finish_table(database(db), place, state, &mut failed)?);
            }
            estimates.push(D1Estimate {
                cap_bytes: D1_CAP_BYTES,
                estimated_bytes: estimate.iter().map(|t| t.estimated_bytes).sum(),
                tables: estimate,
            });
        }
        let mut estimates = estimates.into_iter();
        let estimate = estimates
            .next()
            .ok_or("the writer has no catalogue database")?;
        Ok(D1Written {
            estimate,
            shards: estimates.collect(),
            failed,
        })
    }

    /// Writes, for each table the release before held rows of in each database, the files that
    /// retire the rows this release no longer holds, as the table's parts `r001` on, read from
    /// its row index; and writes out the next row index.
    fn retire(&mut self) -> Result<(), String> {
        let (Some(number), Some(sink)) = (self.comparing, self.sink.as_deref_mut()) else {
            return Ok(());
        };
        for (i, tables) in self.databases.iter_mut().enumerate() {
            for (place, state) in tables.iter_mut().enumerate() {
                if let Some(next) = state.next.as_mut() {
                    next.flush()?;
                }
                let Some(previous) = state.previous.take() else {
                    continue;
                };
                let table = state.table;
                let to = Place {
                    database: database(i),
                    at: place,
                    table: table.name,
                    kind: Kind::Retired,
                };
                let head = format!(
                    "UPDATE {} SET {RETIRED} = {number} WHERE {RETIRED} IS NULL AND release < \
                     {number} AND ({}) IN (VALUES ",
                    table.name,
                    table.natural_key().collect::<Vec<_>>().join(", ")
                );
                let mut statements = Statements::with(head, ");\n");
                let mut parts = parts::Parts::default();
                state.retired = previous.retired(|key| match statements.push_tuple(key) {
                    Some(statement) => parts.push(sink, to, &statement),
                    None => Ok(()),
                })?;
                if let Some(last) = statements.finish() {
                    parts.push(sink, to, &last)?;
                }
                parts.finish(sink, to)?;
            }
        }
        Ok(())
    }

    /// Puts the table's last part in `db` and gives its estimate, adding its failed checks.
    fn finish_table(
        &mut self,
        db: Database,
        place: usize,
        mut state: TableState,
        failed: &mut Vec<Failed>,
    ) -> Result<TableEstimate, String> {
        if let Some(sink) = self.sink.as_deref_mut() {
            let to = Place {
                database: db,
                at: place,
                table: state.table.name,
                kind: Kind::Rows,
            };
            if let Some(last) = state.statements.finish() {
                state.parts.push(sink, to, &last)?;
            }
            state.parts.finish(sink, to)?;
        }
        if let Some(cause) = state.not_finite.take() {
            failed.push(Failed {
                check: D1_FINITE,
                cause: format!("{db}: {cause}"),
            });
        }
        if let Some(first) = &state.first_repeated {
            failed.push(Failed {
                check: D1_KEYS,
                cause: format!(
                    "{db}: table {}: {} row(s) repeat a primary key another row has, from {first}",
                    state.table.name, state.repeated
                ),
            });
        }
        Ok(TableEstimate {
            table: state.table.name.to_owned(),
            rows: state.rows,
            estimated_bytes: state.bytes,
            fingerprint: state.fingerprint.finish(),
            added: self.comparing.map(|_| state.added),
            retired: self.comparing.map(|_| state.retired),
            content_sha256: state.content.map(|mut rows| {
                state.table.sort_by_key(&mut rows);
                content_sha256(&rows)
            }),
        })
    }
}

#[cfg(test)]
mod tests;
