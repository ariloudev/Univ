//! Where a release's objects are (shard-a-releases-objects-across-d1-databases): the catalogue
//! database names the object shard of every object id (`object_shards`), the shards a release
//! uses (`release_shards`) and the level-0 cells of each domain each one holds, as a range of
//! Morton codes (`shard_cells`). A release never changes once published, so its shards and
//! their cells are read once and kept.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use univ_model::release::Cell;
use univ_model::release::shards::morton_clamped;

use crate::backend::{Backend, column, first, rows_as};
use crate::problem::Problem;

/// The object shards of a release and the cells each holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShardMap {
    /// Every shard the release uses, in order.
    pub shards: Vec<u32>,
    runs: Vec<Run>,
}

/// The level-0 cells of one domain one shard holds: those whose codes lie from `first` to
/// `last`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Run {
    shard: u32,
    domain: i64,
    first: u64,
    last: u64,
}

impl ShardMap {
    /// The shards that may hold objects of the domain whose term is `domain` in the cells from
    /// `min` to `max`, in order: those whose range of codes meets the box's.
    #[must_use]
    pub fn meeting(&self, domain: i64, min: Cell, max: Cell) -> Vec<u32> {
        let (lo, hi) = (morton_clamped(min), morton_clamped(max));
        let mut shards: Vec<u32> = self
            .runs
            .iter()
            .filter(|r| r.domain == domain && r.first <= hi && r.last >= lo)
            .map(|r| r.shard)
            .collect();
        shards.sort_unstable();
        shards.dedup();
        shards
    }
}

/// The object shards of `release` and their cells, from the backend's keeping or read in one
/// round trip.
///
/// # Errors
///
/// An internal-error problem when the catalogue database cannot be read.
pub async fn shard_map<B: Backend>(backend: &B, release: u32) -> Result<Arc<ShardMap>, Problem> {
    if let Some(map) = backend.kept().shard_maps.get(&release) {
        return Ok(map);
    }
    let params = [json!(release)];
    let mut batch = backend
        .releases_batch(&[
            (
                "SELECT shard FROM release_shards WHERE release = ?1 ORDER BY shard",
                &params,
            ),
            (
                "SELECT shard, domain, first, last FROM shard_cells WHERE release = ?1",
                &params,
            ),
        ])
        .await?
        .into_iter();
    let shards = batch
        .next()
        .unwrap_or_default()
        .iter()
        .map(|row| column(row, "shard"))
        .collect::<Result<_, _>>()?;
    let runs = rows_as(batch.next().unwrap_or_default())?;
    Ok(backend
        .kept()
        .shard_maps
        .keep(release, ShardMap { shards, runs }))
}

/// The object shard that holds object `id` of `release`, or none when the release does not
/// hold it.
///
/// # Errors
///
/// An internal-error problem when the catalogue database cannot be read.
pub async fn shard_of<B: Backend>(
    backend: &B,
    release: u32,
    id: u64,
) -> Result<Option<u32>, Problem> {
    let rows = backend
        .releases(
            "SELECT shard FROM object_shards WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    first(rows).map(|row| column(&row, "shard")).transpose()
}

#[cfg(test)]
mod tests {
    use univ_model::release::shards::morton;

    use super::*;

    fn cell(ix: i64, iy: i64, iz: i64) -> Cell {
        Cell { ix, iy, iz }
    }

    #[test]
    fn a_box_meets_the_shards_whose_codes_its_own_overlap() {
        let code = |c| morton(c).unwrap();
        let run = |shard, domain, first, last| Run {
            shard,
            domain,
            first: code(first),
            last: code(last),
        };
        let map = ShardMap {
            shards: vec![0, 1, 2],
            runs: vec![
                run(0, 7, cell(-1, -1, -1), cell(0, 0, 0)),
                run(1, 7, cell(1, 0, 0), cell(5, 5, 5)),
                run(2, 7, cell(6, 6, 6), cell(9, 9, 9)),
                run(2, 8, cell(0, 0, 0), cell(0, 0, 0)),
            ],
        };
        assert_eq!(map.meeting(7, cell(0, 0, 0), cell(1, 0, 0)), [0, 1]);
        assert_eq!(map.meeting(7, cell(7, 7, 7), cell(8, 8, 8)), [2]);
        assert_eq!(map.meeting(8, cell(-4, -4, -4), cell(4, 4, 4)), [2]);
        assert_eq!(
            map.meeting(9, cell(0, 0, 0), cell(1, 1, 1)),
            Vec::<u32>::new()
        );
    }
}
