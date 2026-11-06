//! The row index a release keeps for the next (build-a-release): for each row of each table of
//! `Scope::PerRelease` in each database, a hash of its key and of its content, both without its
//! `release`, the release that added it, and its key as SQL literals. The next build compares
//! each row it makes with it: a row whose content is unchanged is not written again and keeps
//! the release that added it; a changed row is written again, valid from the new release; and a
//! row of the index the new release does not make, or makes changed, is retired by its key.
//!
//! The index lives in the state directory, `rows/{database}/{table}.idx`, one file per table,
//! each a run of records: key hash (16 bytes), content hash (16), release (4), key length (4),
//! all little-endian, then the key. A build writes the next release's into `rows.part/` beside
//! the state, and installing the state moves it into the release's state directory.

use std::collections::HashMap;
use std::fs::File;
use std::hash::Hash;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use univ_model::release::shards::Database;
use univ_model::release::{SqlValue, Table};

/// The directory, within a state, of the row index.
pub const ROWS: &str = "rows";
/// The bytes of a record before its key.
const HEAD: usize = 16 + 16 + 4 + 4;

/// A row of the index as a build looks it up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indexed {
    pub key: u128,
    pub content: u128,
    pub release: u32,
}

/// What became of a row of the index in the build comparing with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// Not made by the build: retired.
    Gone,
    /// Made with the same content: kept, not written again.
    Kept,
    /// Made with other content: retired, and written again.
    Changed,
}

/// One table's index of one database, sorted by key, with each row's fate so far.
struct Rows {
    file: PathBuf,
    indexed: Vec<Indexed>,
    fates: Vec<Fate>,
}

/// The row index of the release before, as a build compares its rows with it.
#[derive(Default)]
pub struct Previous {
    tables: HashMap<(Database, &'static str), Rows>,
}

/// How a build is to treat a row it makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compared {
    /// New, or changed: written, valid from the new release.
    Write,
    /// Unchanged: not written, valid from the release that added it.
    Keep { release: u32 },
}

/// The file of `table` of `database` in the index directory `dir`.
fn file_of(dir: &Path, database: Database, table: &str) -> PathBuf {
    dir.join(database.dir()).join(format!("{table}.idx"))
}

/// Reads the records of `file`, giving each with its key's text to `each`.
fn read_records(file: &Path, mut each: impl FnMut(Indexed, String)) -> std::io::Result<()> {
    let mut input = BufReader::new(File::open(file)?);
    let mut head = [0_u8; HEAD];
    loop {
        match input.read_exact(&mut head) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        }
        let word = |at: usize| {
            let mut b = [0_u8; 16];
            b.copy_from_slice(&head[at..at + 16]);
            u128::from_le_bytes(b)
        };
        let small = |at: usize| {
            let mut b = [0_u8; 4];
            b.copy_from_slice(&head[at..at + 4]);
            u32::from_le_bytes(b)
        };
        let mut key = vec![0_u8; small(36) as usize];
        input.read_exact(&mut key)?;
        let row = Indexed {
            key: word(0),
            content: word(16),
            release: small(32),
        };
        each(row, String::from_utf8(key).map_err(std::io::Error::other)?);
    }
}

impl Previous {
    /// The index in `dir` of the tables `tables` of each database it holds.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read.
    pub fn read(dir: &Path, tables: &[&'static Table]) -> std::io::Result<Self> {
        let mut out = Self::default();
        let mut databases = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(database) = entry.file_name().to_str().and_then(Database::from_dir) {
                databases.push(database);
            }
        }
        for database in databases {
            for table in tables {
                let file = file_of(dir, database, table.name);
                if !file.exists() {
                    continue;
                }
                let mut indexed = Vec::new();
                read_records(&file, |row, _| indexed.push(row))?;
                indexed.sort_unstable_by_key(|r| r.key);
                let fates = vec![Fate::Gone; indexed.len()];
                let rows = Rows {
                    file,
                    indexed,
                    fates,
                };
                out.tables.insert((database, table.name), rows);
            }
        }
        Ok(out)
    }

    /// The databases the index holds rows of.
    #[must_use]
    pub fn databases(&self) -> Vec<Database> {
        let mut found: Vec<Database> = self.tables.keys().map(|(d, _)| *d).collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// Compares a row of `table` in `database`, its key and content hashed, with the index.
    pub fn compare(
        &mut self,
        database: Database,
        table: &'static str,
        key: u128,
        content: u128,
    ) -> Compared {
        let Some(rows) = self.tables.get_mut(&(database, table)) else {
            return Compared::Write;
        };
        let Ok(at) = rows.indexed.binary_search_by_key(&key, |r| r.key) else {
            return Compared::Write;
        };
        let row = rows.indexed[at];
        if row.content == content {
            rows.fates[at] = Fate::Kept;
            Compared::Keep {
                release: row.release,
            }
        } else {
            rows.fates[at] = Fate::Changed;
            Compared::Write
        }
    }

    /// The key, as SQL literals, of each row of `table` in `database` that the build retires:
    /// those it did not make or made changed.
    ///
    /// # Errors
    ///
    /// Fails when the index file cannot be read again.
    pub fn retired(&self, database: Database, table: &'static str) -> std::io::Result<Vec<String>> {
        let Some(rows) = self.tables.get(&(database, table)) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        read_records(&rows.file, |row, key| {
            let kept = rows
                .indexed
                .binary_search_by_key(&row.key, |r| r.key)
                .is_ok_and(|at| rows.fates[at] == Fate::Kept);
            if !kept {
                out.push(key);
            }
        })?;
        Ok(out)
    }
}

/// The next release's row index, written as the build makes its rows.
pub struct Next {
    dir: PathBuf,
    files: HashMap<(Database, &'static str), BufWriter<File>>,
}

impl Next {
    /// A new index in `dir`, replacing what a stopped build left there.
    ///
    /// # Errors
    ///
    /// Fails when the directory cannot be made.
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_owned(),
            files: HashMap::new(),
        })
    }

    /// Records a row of `table` in `database`.
    ///
    /// # Errors
    ///
    /// Fails when its file cannot be written.
    pub fn record(
        &mut self,
        database: Database,
        table: &'static str,
        row: Indexed,
        key: &str,
    ) -> std::io::Result<()> {
        if !self.files.contains_key(&(database, table)) {
            let file = file_of(&self.dir, database, table);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            self.files
                .insert((database, table), BufWriter::new(File::create(file)?));
        }
        let Some(out) = self.files.get_mut(&(database, table)) else {
            return Err(std::io::Error::other("the index file was not opened"));
        };
        out.write_all(&row.key.to_le_bytes())?;
        out.write_all(&row.content.to_le_bytes())?;
        out.write_all(&row.release.to_le_bytes())?;
        let len = u32::try_from(key.len()).map_err(std::io::Error::other)?;
        out.write_all(&len.to_le_bytes())?;
        out.write_all(key.as_bytes())
    }

    /// Writes out every file, giving the directory.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be written.
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        for out in self.files.values_mut() {
            out.flush()?;
        }
        Ok(self.dir)
    }
}

/// The places among a table's columns of its key without `release`, and of its other columns
/// without it.
#[must_use]
pub fn places(table: &Table) -> (Vec<usize>, Vec<usize>) {
    let natural = table
        .key
        .iter()
        .filter(|c| **c != "release")
        .filter_map(|c| table.position(c))
        .collect();
    let content = (0..table.columns.len())
        .filter(|&i| table.columns[i] != "release")
        .collect();
    (natural, content)
}

/// A 128-bit hash of the values of `row` at `places`, each with its type.
#[must_use]
pub fn hash_at(places: &[usize], row: &[SqlValue]) -> u128 {
    crate::hash::hash128(|h| {
        for &i in places {
            match row.get(i) {
                Some(SqlValue::Integer(n)) => (1_u8, n).hash(h),
                Some(SqlValue::Real(x)) => (2_u8, x.to_bits()).hash(h),
                Some(SqlValue::Text(t)) => (3_u8, t).hash(h),
                Some(SqlValue::Null) | None => 0_u8.hash(h),
            }
        }
    })
}

/// The values of `row` at `places` as an SQL tuple, `(1, 'a')`.
#[must_use]
pub fn tuple_at(places: &[usize], row: &[SqlValue]) -> String {
    let values: Vec<String> = places
        .iter()
        .filter_map(|&i| row.get(i).map(SqlValue::literal))
        .collect();
    format!("({})", values.join(", "))
}

#[cfg(test)]
mod tests {
    use univ_model::release::rows::ObjectRow;
    use univ_model::release::{Row, TABLES};

    use super::*;

    fn objects() -> &'static Table {
        TABLES
            .iter()
            .find(|t| t.name == ObjectRow::TABLE.name)
            .unwrap()
    }

    #[test]
    fn an_index_finds_kept_changed_and_gone_rows_by_their_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut next = Next::new(dir.path()).unwrap();
        let db = Database::Shard(2);
        for (key, content) in [(5_u128, 50_u128), (1, 10), (9, 90)] {
            let row = Indexed {
                key,
                content,
                release: 3,
            };
            next.record(db, "objects", row, &format!("({key})"))
                .unwrap();
        }
        let dir = next.finish().unwrap();
        let mut previous = Previous::read(&dir, &[objects()]).unwrap();
        assert_eq!(previous.databases(), [db]);
        assert_eq!(
            previous.compare(db, "objects", 1, 10),
            Compared::Keep { release: 3 }
        );
        assert_eq!(previous.compare(db, "objects", 5, 51), Compared::Write);
        assert_eq!(previous.compare(db, "objects", 7, 70), Compared::Write);
        // 5 changed and 9 gone are retired; 1 is kept.
        let mut retired = previous.retired(db, "objects").unwrap();
        retired.sort();
        assert_eq!(retired, ["(5)", "(9)"]);
        let none: Vec<String> = Vec::new();
        assert_eq!(
            previous.retired(Database::Catalogue, "objects").unwrap(),
            none
        );
    }

    #[test]
    fn a_key_and_content_leave_out_the_release() {
        let table = objects();
        let (natural, content) = places(table);
        assert_eq!(natural, [table.position("id").unwrap()]);
        assert!(!content.contains(&table.position("release").unwrap()));
        let mut row = vec![SqlValue::Null; table.columns.len()];
        row[table.position("id").unwrap()] = SqlValue::Integer(7);
        row[table.position("release").unwrap()] = SqlValue::Integer(1);
        let before = (hash_at(&natural, &row), hash_at(&content, &row));
        row[table.position("release").unwrap()] = SqlValue::Integer(2);
        assert_eq!((hash_at(&natural, &row), hash_at(&content, &row)), before);
        assert_eq!(tuple_at(&natural, &row), "(7)");
    }
}
