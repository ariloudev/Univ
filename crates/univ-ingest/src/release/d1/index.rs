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
//!
//! The hashes are SHA-256's first 16 bytes, of each value tagged by its type: a fixed algorithm,
//! so that a build compares with an index any earlier toolchain wrote.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use univ_model::release::shards::Database;
use univ_model::release::{SqlValue, Table};

/// The directory, within a state, of the row index.
pub const ROWS: &str = "rows";
/// The bytes of a record before its key.
const HEAD: usize = 16 + 16 + 4 + 4;
/// The records of the next index a table holds before they are appended to its file.
const BUFFERED: usize = 1 << 20;
/// The buffer an index file is read through.
const READ_BUFFER: usize = 1 << 20;

/// The file of `table` of `database` in the index directory `dir`.
fn file_of(dir: &Path, database: Database, table: &str) -> PathBuf {
    dir.join(database.dir()).join(format!("{table}.idx"))
}

/// A row of the index: its key and content hashes and the release that added it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Indexed {
    pub key: u128,
    pub content: u128,
    pub release: u32,
}

/// An index file read record by record, its keys read only when asked for.
struct Records {
    input: BufReader<File>,
    /// The bytes of the file not yet read or skipped.
    left: u64,
    key_len: usize,
    key: Vec<u8>,
}

impl Records {
    fn open(file: &Path) -> std::io::Result<Self> {
        let input = File::open(file)?;
        let left = input.metadata()?.len();
        Ok(Self {
            input: BufReader::with_capacity(READ_BUFFER, input),
            left,
            key_len: 0,
            key: Vec::new(),
        })
    }

    /// The next record, its key left unread: [`Records::key`] reads it, else the next call
    /// skips it.
    fn next(&mut self) -> std::io::Result<Option<Indexed>> {
        if self.key_len > 0 {
            let skip = i64::try_from(self.key_len).map_err(std::io::Error::other)?;
            self.input.seek_relative(skip)?;
            self.key_len = 0;
        }
        // The file ends between records; a record cut short is an error, not its end.
        if self.left == 0 {
            return Ok(None);
        }
        let mut head = [0_u8; HEAD];
        self.input
            .read_exact(&mut head)
            .map_err(|e| cut_short(&e))?;
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
        self.key_len = small(36) as usize;
        let record = (HEAD + self.key_len) as u64;
        if record > self.left {
            return Err(cut_short(&std::io::ErrorKind::UnexpectedEof.into()));
        }
        self.left -= record;
        Ok(Some(Indexed {
            key: word(0),
            content: word(16),
            release: small(32),
        }))
    }

    /// The key of the record [`Records::next`] gave.
    fn key(&mut self) -> std::io::Result<&str> {
        self.key.resize(self.key_len, 0);
        self.input.read_exact(&mut self.key)?;
        self.key_len = 0;
        std::str::from_utf8(&self.key).map_err(std::io::Error::other)
    }
}

/// The error of a record the file ends within.
fn cut_short(e: &std::io::Error) -> std::io::Error {
    std::io::Error::new(
        e.kind(),
        format!("a record cut short, the file truncated: {e}"),
    )
}

/// The low 64 bits of a hash.
#[expect(clippy::cast_possible_truncation, reason = "the low half, by intent")]
fn low(hash: u128) -> u64 {
    hash as u64
}

/// One table's index of the release before in one database: its rows in the order the build
/// wrote them, and which of them this build keeps.
pub(super) struct Previous {
    file: PathBuf,
    keys: Vec<u128>,
    /// The low half of each content hash: compared only with the row of the same key.
    contents: Vec<u64>,
    releases: Vec<u32>,
    kept: Vec<bool>,
    /// The rows' places, ordered by key, for a row not met in order.
    by_key: Vec<u32>,
    /// Where the next row in order is expected: a build writes its rows in the order the one
    /// before did, so nearly every row is found here without a search.
    cursor: usize,
}

impl Previous {
    /// The index of `table` in `database` in `dir`, none when it holds no file of it.
    pub(super) fn read(
        dir: &Path,
        database: Database,
        table: &str,
    ) -> std::io::Result<Option<Self>> {
        let file = file_of(dir, database, table);
        if !file.exists() {
            return Ok(None);
        }
        let (mut keys, mut contents, mut releases) = (Vec::new(), Vec::new(), Vec::new());
        let mut records = Records::open(&file)?;
        while let Some(row) = records.next()? {
            keys.push(row.key);
            contents.push(low(row.content));
            releases.push(row.release);
        }
        let n = u32::try_from(keys.len()).map_err(std::io::Error::other)?;
        let mut by_key: Vec<u32> = (0..n).collect();
        by_key.sort_unstable_by_key(|&i| keys[i as usize]);
        Ok(Some(Self {
            file,
            kept: vec![false; keys.len()],
            keys,
            contents,
            releases,
            by_key,
            cursor: 0,
        }))
    }

    /// The databases `dir` holds an index of.
    pub(super) fn databases(dir: &Path) -> std::io::Result<Vec<Database>> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            if let Some(d) = entry?.file_name().to_str().and_then(Database::from_dir) {
                found.push(d);
            }
        }
        found.sort_unstable();
        Ok(found)
    }

    /// Compares a row, its key and content hashed, with the index: the release that added it
    /// when it is there unchanged, which the build keeps, else none.
    pub(super) fn compare(&mut self, key: u128, content: u128) -> Option<u32> {
        let at = if self.keys.get(self.cursor) == Some(&key) {
            self.cursor
        } else {
            let found = self
                .by_key
                .binary_search_by_key(&key, |&i| self.keys[i as usize])
                .ok()?;
            self.by_key[found] as usize
        };
        self.cursor = at + 1;
        (self.contents[at] == low(content)).then(|| {
            self.kept[at] = true;
            self.releases[at]
        })
    }

    /// Gives `each` the key, as SQL literals, of each row the build retires: those it did not
    /// make or made changed. Gives how many.
    pub(super) fn retired(
        &self,
        mut each: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<u64, String> {
        let problem = |e: std::io::Error| format!("{}: {e}", self.file.display());
        let mut records = Records::open(&self.file).map_err(problem)?;
        let (mut at, mut retired) = (0, 0);
        while records.next().map_err(problem)?.is_some() {
            if !self.kept.get(at).copied().unwrap_or(true) {
                each(records.key().map_err(problem)?)?;
                retired += 1;
            }
            at += 1;
        }
        Ok(retired)
    }
}

/// One table's next index in one database, buffered and appended to its file.
pub(super) struct Next {
    file: PathBuf,
    buffer: Vec<u8>,
}

impl Next {
    /// The next index of `table` in `database`, in `dir`.
    pub(super) fn new(dir: &Path, database: Database, table: &str) -> Self {
        Self {
            file: file_of(dir, database, table),
            buffer: Vec::new(),
        }
    }

    /// Records a row, its key as SQL literals.
    pub(super) fn record(&mut self, row: Indexed, key: &str) -> Result<(), String> {
        self.buffer.extend_from_slice(&row.key.to_le_bytes());
        self.buffer.extend_from_slice(&row.content.to_le_bytes());
        self.buffer.extend_from_slice(&row.release.to_le_bytes());
        let len = u32::try_from(key.len()).map_err(|e| e.to_string())?;
        self.buffer.extend_from_slice(&len.to_le_bytes());
        self.buffer.extend_from_slice(key.as_bytes());
        if self.buffer.len() >= BUFFERED {
            self.flush()?;
        }
        Ok(())
    }

    /// Appends what is buffered to the file: opened only for it, so that the tables of many
    /// databases hold no file open.
    pub(super) fn flush(&mut self) -> Result<(), String> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let problem = |e: std::io::Error| format!("{}: {e}", self.file.display());
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(problem)?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .and_then(|mut f| f.write_all(&self.buffer))
            .map_err(problem)?;
        self.buffer.clear();
        Ok(())
    }
}

/// Makes `dir` an empty directory for a new index, replacing what a stopped build left there.
///
/// # Errors
///
/// Fails when the directory cannot be made.
pub(super) fn fresh(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)
}

/// The places among a table's columns of its key, without `release` for a table whose rows
/// releases share, and of its other columns but `release`.
#[must_use]
pub(super) fn places(table: &Table, shared: bool) -> (Vec<usize>, Vec<usize>) {
    let key = if shared {
        table
            .natural_key()
            .filter_map(|c| table.position(c))
            .collect()
    } else {
        table.key.iter().filter_map(|c| table.position(c)).collect()
    };
    let content = (0..table.columns.len())
        .filter(|&i| table.columns[i] != "release")
        .collect();
    (key, content)
}

/// A 128-bit hash of the values of `row` at `places`, each tagged by its type, a text by its
/// length too: SHA-256's first 16 bytes.
#[must_use]
pub(super) fn hash_at(places: &[usize], row: &[SqlValue]) -> u128 {
    let mut h = Sha256::new();
    for &i in places {
        match row.get(i) {
            Some(SqlValue::Integer(n)) => {
                h.update([1]);
                h.update(n.to_le_bytes());
            }
            Some(SqlValue::Real(x)) => {
                h.update([2]);
                h.update(x.to_bits().to_le_bytes());
            }
            Some(SqlValue::Text(t)) => {
                h.update([3]);
                h.update((t.len() as u64).to_le_bytes());
                h.update(t.as_bytes());
            }
            Some(SqlValue::Null) | None => h.update([0]),
        }
    }
    let mut b = [0_u8; 16];
    b.copy_from_slice(&h.finalize()[..16]);
    u128::from_le_bytes(b)
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
        let db = Database::Shard(2);
        let mut next = Next::new(dir.path(), db, "objects");
        for (key, content) in [(5_u128, 50_u128), (1, 10), (9, 90)] {
            let row = Indexed {
                key,
                content,
                release: 3,
            };
            next.record(row, &format!("({key})")).unwrap();
        }
        next.flush().unwrap();
        assert_eq!(Previous::databases(dir.path()).unwrap(), [db]);
        assert!(
            Previous::read(dir.path(), Database::Catalogue, "objects")
                .unwrap()
                .is_none()
        );
        let mut previous = Previous::read(dir.path(), db, "objects").unwrap().unwrap();
        // 1 met out of order, by its key; 5 changed; 7 new; 9 gone.
        assert_eq!(previous.compare(1, 10), Some(3));
        assert_eq!(previous.compare(5, 51), None);
        assert_eq!(previous.compare(7, 70), None);
        let mut retired = Vec::new();
        let n = previous
            .retired(|k| {
                retired.push(k.to_owned());
                Ok(())
            })
            .unwrap();
        assert_eq!((n, retired), (2, vec!["(5)".to_owned(), "(9)".to_owned()]));

        // An index cut short in a record is refused, not read as ending there.
        let file = file_of(dir.path(), db, "objects");
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 3]).unwrap();
        assert!(Previous::read(dir.path(), db, "objects").is_err());
        std::fs::write(&file, &bytes[..HEAD + 3 + 10]).unwrap();
        assert!(Previous::read(dir.path(), db, "objects").is_err());
    }

    #[test]
    fn a_key_and_content_leave_out_the_release_and_hash_by_a_fixed_algorithm() {
        let table = objects();
        let (natural, content) = places(table, true);
        assert_eq!(natural, [table.position("id").unwrap()]);
        assert!(!content.contains(&table.position("release").unwrap()));
        let mut row = vec![SqlValue::Null; table.columns.len()];
        row[table.position("id").unwrap()] = SqlValue::Integer(7);
        row[table.position("release").unwrap()] = SqlValue::Integer(1);
        let before = (hash_at(&natural, &row), hash_at(&content, &row));
        row[table.position("release").unwrap()] = SqlValue::Integer(2);
        assert_eq!((hash_at(&natural, &row), hash_at(&content, &row)), before);
        // SHA-256 of the tag 1 and 7 as 8 bytes: what any build, on any toolchain, gives.
        let digest = Sha256::digest([1, 7, 0, 0, 0, 0, 0, 0, 0]);
        let mut b = [0_u8; 16];
        b.copy_from_slice(&digest[..16]);
        assert_eq!(hash_at(&natural, &row), u128::from_le_bytes(b));
    }
}
