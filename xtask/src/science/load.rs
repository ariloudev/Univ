//! A built release's D1 rows loaded into a scratch `SQLite` database, and the SQL functions
//! the checks' queries call.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use univ_model::cosmology::Cosmology;
use univ_model::derive::ComovingTable;
use univ_model::position::{separation_degrees, unit_vector};
use univ_model::release::ReleaseManifest;
use univ_model::release::manifest::Target;

/// The scratch database: in memory, or at `file` when one is given. A file that exists already
/// is taken as loaded and opened as it is.
///
/// # Errors
///
/// Fails when the database cannot be opened or set up for a bulk load.
pub fn open(file: Option<&Path>) -> Result<(Connection, bool), String> {
    let loaded = file.is_some_and(Path::exists);
    let db = match file {
        Some(path) => Connection::open(path),
        None => Connection::open_in_memory(),
    }
    .map_err(|e| format!("cannot open the scratch database: {e}"))?;
    // A scratch database: nothing to recover on a crash, so no journal and no syncs.
    db.execute_batch(
        "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY; \
         PRAGMA cache_size = -1048576; PRAGMA locking_mode = EXCLUSIVE;",
    )
    .map_err(|e| format!("cannot set up the scratch database: {e}"))?;
    Ok((db, loaded))
}

/// Executes the release's D1 files, in the order the manifest lists them, in one transaction.
///
/// # Errors
///
/// Fails on a file that cannot be read or executed, naming it.
pub fn load(db: &Connection, dir: &Path, manifest: &ReleaseManifest) -> Result<usize, String> {
    let files: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| f.target == Target::D1)
        .collect();
    if files.is_empty() {
        return Err("the manifest lists no D1 files".to_owned());
    }
    db.execute_batch("BEGIN")
        .map_err(|e| format!("cannot begin the load: {e}"))?;
    for file in &files {
        let path = dir.join(&file.path);
        let sql = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        db.execute_batch(&sql)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    db.execute_batch("COMMIT")
        .map_err(|e| format!("cannot commit the load: {e}"))?;
    Ok(files.len())
}

/// A name-to-number map of one release's `terms` or `sources`.
fn numbers(db: &Connection, sql: &str, release: u32) -> Result<HashMap<String, i64>, String> {
    let mut q = db.prepare(sql).map_err(|e| e.to_string())?;
    let rows = q
        .query_map([release], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// The error a function gives for an argument it cannot use.
fn user_error(message: String) -> rusqlite::Error {
    rusqlite::Error::UserFunctionError(message.into())
}

/// Registers the functions the checks' queries call:
///
/// - `term(name)`: the release's id of a term, such as `term('redshift')`;
/// - `source(id)`: the release's number of a source, such as `source('twomrs')`;
/// - `sep_arcmin(ra1, dec1, ra2, dec2)`: the angle between two directions, arcmin;
/// - `comoving_mpc(z)`: the comoving distance of redshift `z` under the release cosmology.
///
/// `term` and `source` fail on a name the release does not hold, so that a misspelt name is
/// an error rather than a check that selects nothing.
///
/// # Errors
///
/// Fails when the release's terms or sources cannot be read or a function cannot be registered.
pub fn register(db: &Connection, release: u32, cosmology: &Cosmology) -> Result<(), String> {
    let terms = numbers(db, "SELECT term, id FROM terms WHERE release = ?1", release)?;
    let sources = numbers(db, "SELECT id, n FROM sources WHERE release = ?1", release)?;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    let lookup = |name: &'static str, map: HashMap<String, i64>| {
        db.create_scalar_function(name, 1, flags, move |ctx| {
            let key: String = ctx.get(0)?;
            map.get(&key)
                .copied()
                .ok_or_else(|| user_error(format!("the release holds no {name} {key:?}")))
        })
    };
    lookup("term", terms).map_err(|e| e.to_string())?;
    lookup("source", sources).map_err(|e| e.to_string())?;
    db.create_scalar_function("sep_arcmin", 4, flags, |ctx| {
        let (ra1, dec1, ra2, dec2): (f64, f64, f64, f64) =
            (ctx.get(0)?, ctx.get(1)?, ctx.get(2)?, ctx.get(3)?);
        Ok(60.0 * separation_degrees(unit_vector(ra1, dec1), unit_vector(ra2, dec2)))
    })
    .map_err(|e| e.to_string())?;
    // Tabulated as the release places distances: within 1e-9 of the integral.
    let table = ComovingTable::new(cosmology);
    db.create_scalar_function("comoving_mpc", 1, flags, move |ctx| {
        let z: f64 = ctx.get(0)?;
        table.distance(z).map_err(|e| user_error(e.to_string()))
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}
