//! `univ-ingest reindex`: the state's row index and numbering of the release `latest` points to,
//! made again from the local D1 it was published to.

use std::path::Path;

use crate::release::local_d1::LocalD1;
use crate::release::reindex::reindex;
use crate::state::{State, current_dir};

/// Writes into the state directory `state`'s current state the row index and numbering of the
/// release `latest` points to in the local D1 of Wrangler's state directory `persist_to`, and
/// says what it wrote.
///
/// # Errors
///
/// Fails when the local D1 cannot be read or is older than schema 6, when `latest` points to a
/// release other than the state's last, or when a file cannot be written.
pub fn reindex_command(state: &Path, persist_to: &Path) -> Result<String, String> {
    let held = State::read(state).map_err(|e| e.to_string())?;
    let last = u32::try_from(held.ledger.releases().len()).map_err(|e| e.to_string())?;
    let local = LocalD1::open(persist_to)?;
    if local.latest != last {
        return Err(format!(
            "latest points to release {} in the local D1, and the state's last release is \
             {last}: the index must be of the release the next build follows",
            local.latest
        ));
    }
    let dir = current_dir(state).map_err(|e| e.to_string())?;
    let done = reindex(&local, local.latest, &dir)?;
    let rows: u64 = done.rows.values().sum();
    Ok(format!(
        "release {last}: {rows} rows of {} tables indexed in {}; numbering of {} terms, {} \
         sources, {} chains and {} object shards",
        done.rows.len(),
        dir.display(),
        done.kept.terms.len(),
        done.kept.sources.len(),
        done.kept.chains.len(),
        done.kept.cells.len(),
    ))
}
