//! A release's D1 rows written as SQL files ready for `wrangler d1 execute --file`, one row at a
//! time, with the estimate of the storage they take, each table's fingerprint and the checks of
//! its rows: nothing keeps a table's rows.
//!
//! The files are `d1/0000-schema.sql`, the schema and its version, then
//! `d1/{nnnn}-{table}-{part}.sql` for each table that has rows, `nnnn` its place in
//! `univ_model::release::TABLES`, each part at most [`MAX_FILE_BYTES`]. Executed in path order
//! they load the release; each holds `INSERT OR IGNORE` statements only (D1 runs a file without
//! explicit transactions), so a file executed twice changes nothing. The writer checks that each
//! table's primary keys are unique and every number finite, so `INSERT OR IGNORE` can only skip
//! a row written before, identical, and no NULL stands for a number.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use sha2::{Digest, Sha256};
use univ_model::release::fingerprint::canonical_row;
use univ_model::release::manifest::{D1_CAP_BYTES, D1Estimate, TableEstimate};
use univ_model::release::{
    Fingerprinter, Row, SCHEMA, SCHEMA_VERSION, SqlValue, Statements, TABLES, Table, keys,
};

use super::check::{D1_FINITE, D1_KEYS, Failed};

/// The largest D1 file written, unless one statement alone is larger.
pub const MAX_FILE_BYTES: usize = 32 << 20;

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

/// One table's rows as they are written.
struct TableState {
    table: Table,
    statements: Statements,
    part: String,
    parts: usize,
    rows: u64,
    bytes: u64,
    fingerprint: Fingerprinter,
    /// The places of the key's columns among the table's.
    key: Vec<usize>,
    /// The rows of a table checked by its content, kept for their digest: a few hundred.
    content: Option<Vec<Vec<SqlValue>>>,
    keys: HashSet<u128>,
    repeated: u64,
    first_repeated: Option<String>,
    not_finite: Option<String>,
}

impl TableState {
    fn new(table: Table) -> Self {
        Self {
            table,
            statements: Statements::new(table),
            part: String::new(),
            parts: 0,
            rows: 0,
            bytes: 0,
            fingerprint: Fingerprinter::new(table),
            key: table.key.iter().filter_map(|c| table.position(c)).collect(),
            content: table.checked_by_content().then(Vec::new),
            keys: HashSet::new(),
            repeated: 0,
            first_repeated: None,
            not_finite: None,
        }
    }
}

/// A row's primary key, as its columns and SQL literals.
fn key_of(table: &Table, row: &[SqlValue]) -> String {
    let parts: Vec<String> = table
        .key
        .iter()
        .filter_map(|c| {
            let v = table.position(c).and_then(|i| row.get(i))?;
            Some(format!("{c} = {}", v.literal()))
        })
        .collect();
    format!("({})", parts.join(", "))
}

/// A 128-bit hash of the key values of `row` at `places`, two `SipHash` runs apart, each value
/// hashed with its type: a key is told from another by it, and no text is made of it.
fn hash128(places: &[usize], row: &[SqlValue]) -> u128 {
    let mut a = DefaultHasher::new();
    let mut b = DefaultHasher::new();
    0x5eed_u32.hash(&mut b);
    for &i in places {
        for h in [&mut a, &mut b] {
            match row.get(i) {
                Some(SqlValue::Integer(n)) => (1_u8, n).hash(h),
                Some(SqlValue::Real(x)) => (2_u8, x.to_bits()).hash(h),
                Some(SqlValue::Text(t)) => (3_u8, t).hash(h),
                Some(SqlValue::Null) | None => 0_u8.hash(h),
            }
        }
    }
    (u128::from(a.finish()) << 64) | u128::from(b.finish())
}

/// The place in `TABLES` of the table named `name`, worked out when compiled for a row type.
const fn place_of(name: &str) -> usize {
    let mut i = 0;
    while i < TABLES.len() {
        if same(TABLES[i].name.as_bytes(), name.as_bytes()) {
            return i;
        }
        i += 1;
    }
    TABLES.len()
}

const fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
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

/// The path of part `part` (from 1) of the table at `place` (from 0) in `TABLES`.
fn part_path(place: usize, table: &str, part: usize) -> String {
    format!("{}/{:04}-{table}-{part:03}.sql", keys::D1_DIR, place + 1)
}

/// The D1 files of a release, written as its rows come.
pub struct D1Writer<'s> {
    sink: &'s mut dyn PartSink,
    tables: Vec<TableState>,
}

/// What the writer made of a release's rows once every file is written.
#[derive(Debug)]
pub struct D1Written {
    pub estimate: D1Estimate,
    /// The checks of the rows that failed.
    pub failed: Vec<Failed>,
}

impl<'s> D1Writer<'s> {
    /// A writer into `sink`, which takes the schema's file first.
    ///
    /// # Errors
    ///
    /// Fails when the sink cannot take it.
    pub fn new(sink: &'s mut dyn PartSink) -> Result<Self, String> {
        let schema = format!(
            "{SCHEMA}\nINSERT OR IGNORE INTO schema_version (version) VALUES ({SCHEMA_VERSION});\n"
        );
        sink.put(
            format!("{}/0000-schema.sql", keys::D1_DIR),
            schema.as_bytes(),
        )?;
        Ok(Self {
            sink,
            tables: TABLES.iter().copied().map(TableState::new).collect(),
        })
    }

    /// Writes `row`.
    ///
    /// # Errors
    ///
    /// Fails when its table is not of the schema or a file cannot be put.
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

    /// Writes a row of `table`, the table at `place` in `TABLES`.
    fn add_at(&mut self, place: usize, table: &Table, values: &[SqlValue]) -> Result<(), String> {
        let state = self
            .tables
            .get_mut(place)
            .ok_or_else(|| format!("{} is not a table of the schema", table.name))?;
        state.rows += 1;
        state.bytes += table.estimated_bytes(values);
        state.fingerprint.add(values);
        if let Some(content) = &mut state.content {
            content.push(values.to_vec());
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
        if !state.keys.insert(hash128(&state.key, values)) {
            state.repeated += 1;
            if state.first_repeated.is_none() {
                state.first_repeated = Some(key_of(table, values));
            }
        }
        if let Some(statement) = state.statements.push(values) {
            Self::to_part(&mut *self.sink, place, state, &statement)?;
        }
        Ok(())
    }

    /// Adds `statement` to the table's current part, putting the part first when it would grow
    /// beyond [`MAX_FILE_BYTES`].
    fn to_part(
        sink: &mut dyn PartSink,
        place: usize,
        state: &mut TableState,
        statement: &str,
    ) -> Result<(), String> {
        if !state.part.is_empty() && state.part.len() + statement.len() > MAX_FILE_BYTES {
            state.parts += 1;
            sink.put(
                part_path(place, state.table.name, state.parts),
                state.part.as_bytes(),
            )?;
            state.part.clear();
        }
        state.part.push_str(statement);
        Ok(())
    }

    /// Forgets the keys seen of `tables`, whose keys hold the object id, once an object's rows
    /// are all written: their keys are unique if unique within each object, the object's id
    /// being unique among the objects.
    pub fn forget_keys(&mut self, tables: &[&str]) {
        for state in &mut self.tables {
            if tables.contains(&state.table.name) {
                state.keys.clear();
            }
        }
    }

    /// The estimated D1 bytes of every row written so far.
    #[must_use]
    pub fn estimated_bytes(&self) -> u64 {
        self.tables.iter().map(|t| t.bytes).sum()
    }

    /// The rows written so far to the table `name`.
    #[must_use]
    pub fn rows(&self, name: &str) -> u64 {
        self.tables
            .iter()
            .find(|t| t.table.name == name)
            .map_or(0, |t| t.rows)
    }

    /// Puts each table's last part, and gives the estimate and the failed checks.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be put.
    pub fn finish(self) -> Result<D1Written, String> {
        let mut tables = Vec::with_capacity(self.tables.len());
        let mut failed = Vec::new();
        for (place, mut state) in self.tables.into_iter().enumerate() {
            if let Some(last) = state.statements.finish() {
                Self::to_part(self.sink, place, &mut state, &last)?;
            }
            if !state.part.is_empty() {
                state.parts += 1;
                self.sink.put(
                    part_path(place, state.table.name, state.parts),
                    state.part.as_bytes(),
                )?;
                state.part = String::new();
            }
            if let Some(cause) = state.not_finite.take() {
                failed.push(Failed {
                    check: D1_FINITE,
                    cause,
                });
            }
            if let Some(first) = &state.first_repeated {
                failed.push(Failed {
                    check: D1_KEYS,
                    cause: format!(
                        "table {}: {} row(s) repeat a primary key another row has, from {first}",
                        state.table.name, state.repeated
                    ),
                });
            }
            tables.push(TableEstimate {
                table: state.table.name.to_owned(),
                rows: state.rows,
                estimated_bytes: state.bytes,
                fingerprint: state.fingerprint.finish(),
                content_sha256: state.content.map(|mut rows| {
                    state.table.sort_by_key(&mut rows);
                    content_sha256(&rows)
                }),
            });
        }
        let estimate = D1Estimate {
            cap_bytes: D1_CAP_BYTES,
            estimated_bytes: tables.iter().map(|t| t.estimated_bytes).sum(),
            tables,
        };
        Ok(D1Written { estimate, failed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use univ_model::release::rows::{IdReplacedRow, IdsIssuedRow, ReleaseRow};

    #[test]
    fn the_files_load_the_schema_then_each_table_in_order() {
        let mut sink = InMemory::default();
        let mut writer = D1Writer::new(&mut sink).unwrap();
        writer
            .add(&IdsIssuedRow {
                release: 1,
                first: 1,
                last: 9,
            })
            .unwrap();
        writer.add_all::<ReleaseRow>(&[]).unwrap();
        let rows = writer.rows("ids_issued");
        let estimate = writer.finish().unwrap().estimate;
        let files = sink.files;
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["d1/0000-schema.sql", "d1/0012-ids_issued-001.sql"]);
        let text = String::from_utf8(files[1].1.clone()).unwrap();
        assert_eq!(
            text,
            "INSERT OR IGNORE INTO ids_issued (release, first, last) VALUES\n(1, 1, 9);\n"
        );
        assert_eq!(estimate.tables.len(), TABLES.len());
        assert!(estimate.estimated_bytes > 0 && estimate.estimated_bytes < 100);
        assert_eq!(rows, 1);
    }

    #[test]
    fn a_table_is_split_into_parts_and_its_keys_and_numbers_checked() {
        let mut sink = InMemory::default();
        let mut writer = D1Writer::new(&mut sink).unwrap();
        let replaced = |id| IdReplacedRow {
            id,
            release: 1,
            change: "retired".into(),
            replaced_by: "x".repeat(20_000),
        };
        // About 20 kB a row: 2,000 rows fill more than one 32 MiB part.
        for id in 0..2_000 {
            writer.add(&replaced(id)).unwrap();
        }
        writer.add(&replaced(7)).unwrap();
        let mut not_finite = ReleaseRow {
            number: 1,
            built_at: String::new(),
            cosmology: String::new(),
            peculiar_velocity_km_s: f64::NAN,
            model_version: String::new(),
            ingest_version: String::new(),
            schema_version: 2,
            matching_rules: String::new(),
            ranking_rule: String::new(),
            ranking_rule_name: String::new(),
            sources: String::new(),
            objects: 0,
            structures: 0,
            grids: 0,
            tiles: 0,
            d1_estimated_bytes: 0,
        };
        writer.add(&not_finite).unwrap();
        not_finite.number = 2;
        writer.add(&not_finite).unwrap();
        let written = writer.finish().unwrap();
        // A part is put once full; the rest when the writer finishes, in table order.
        let mut parts: Vec<&String> = sink.files.iter().map(|(p, _)| p).collect();
        parts.sort();
        assert_eq!(
            parts[1..],
            [
                "d1/0001-releases-001.sql",
                "d1/0013-ids_replaced-001.sql",
                "d1/0013-ids_replaced-002.sql"
            ]
        );
        assert!(sink.files.iter().all(|(_, b)| b.len() <= MAX_FILE_BYTES));
        let repeated = written.failed.iter().find(|f| f.check == D1_KEYS).unwrap();
        assert!(
            repeated.cause.contains("1 row(s) repeat"),
            "{}",
            repeated.cause
        );
        assert!(repeated.cause.contains("(id = 7)"), "{}", repeated.cause);
        let nan = written
            .failed
            .iter()
            .find(|f| f.check == D1_FINITE)
            .unwrap();
        assert_eq!(
            nan.cause,
            "table releases, row (number = 1), column peculiar_velocity_km_s: NaN is not a \
             finite number",
            "the first only"
        );
        let table = &written.estimate.tables[12];
        assert_eq!((table.table.as_str(), table.rows), ("ids_replaced", 2_001));
    }
}
