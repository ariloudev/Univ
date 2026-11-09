//! Where a release's objects are (shard-a-releases-objects-across-d1-databases): the catalogue
//! database names the object shard of every object id (`object_shards`), the shards a release
//! uses (`release_shards`) and the level-0 cells of each domain each one holds, as a range of
//! Morton codes (`shard_cells`). A release never changes once published, so its shards and
//! their cells are read once and kept.
//!
//! A box is compared with a shard's range by the box's own ranges of codes
//! ([`morton_ranges`]), at most [`BOX_RANGES`] of them, so that a box across an index of 0 does
//! not take every shard.

use std::sync::Arc;

use serde_json::json;
use univ_model::release::Cell;
use univ_model::release::rows::ShardCellRow;
use univ_model::release::shards::morton_ranges;
use univ_model::valid_in;

use crate::backend::{Backend, column, first, rows_as};
use crate::problem::Problem;

/// The most ranges of codes a box is cut into to find the shards it meets: a few hundred
/// comparisons, made once a page.
pub const BOX_RANGES: usize = 64;

/// The object shards of a release and the cells each holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShardMap {
    /// Every shard the release uses, in order.
    pub shards: Vec<u32>,
    /// Each shard's range of the level-0 cells of each domain.
    runs: Vec<ShardCellRow>,
}

impl ShardMap {
    /// The shards that may hold objects of the domain whose term is `domain` in the cells from
    /// `min` to `max`, in order: those whose range of codes meets the box's.
    #[must_use]
    pub fn meeting(&self, domain: i64, min: Cell, max: Cell) -> Vec<u32> {
        let ranges = morton_ranges(min, max, BOX_RANGES);
        let meets = |r: &ShardCellRow| {
            i64::from(r.domain) == domain
                && ranges.iter().any(|&(lo, hi)| r.first <= hi && r.last >= lo)
        };
        let mut shards: Vec<u32> = self
            .runs
            .iter()
            .filter(|r| meets(r))
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
                concat!(
                    "SELECT shard FROM release_shards WHERE ",
                    valid_in!("", "?1"),
                    " ORDER BY shard"
                ),
                &params,
            ),
            (
                concat!("SELECT * FROM shard_cells WHERE ", valid_in!("", "?1")),
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
        .releases(SHARD_OF, &[json!(release), json!(id)])
        .await?;
    shard_in(rows)
}

/// The statement that reads the object shard of object `?2` of release `?1`.
pub const SHARD_OF: &str = concat!(
    "SELECT shard FROM object_shards WHERE id = ?2 AND ",
    valid_in!("", "?1")
);

/// The shard [`SHARD_OF`]'s rows name; none when they are none.
///
/// # Errors
///
/// An internal-error problem when the row holds no shard.
pub fn shard_in(rows: Vec<crate::backend::Row>) -> Result<Option<u32>, Problem> {
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
        let run = |shard, domain, first, last| ShardCellRow {
            release: 2,
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
                run(3, 7, cell(30, -30, 30), cell(31, -30, 31)),
            ],
        };
        assert_eq!(map.meeting(7, cell(0, 0, 0), cell(1, 0, 0)), [0, 1]);
        assert_eq!(map.meeting(7, cell(7, 7, 7), cell(8, 8, 8)), [2]);
        assert_eq!(map.meeting(8, cell(-4, -4, -4), cell(4, 4, 4)), [2]);
        // Across 0: shard 3's cells lie between the codes of the box's corners, outside it.
        let (lo, hi) = (code(cell(-2, -2, -2)), code(cell(2, 2, 2)));
        assert!((lo..=hi).contains(&code(cell(30, -30, 30))));
        assert_eq!(map.meeting(7, cell(-2, -2, -2), cell(2, 2, 2)), [0, 1]);
        assert_eq!(
            map.meeting(9, cell(0, 0, 0), cell(1, 1, 1)),
            Vec::<u32>::new()
        );
    }
}
