//! A release's objects divided among object shards (`univ_model::release::shards`): the level-0
//! cells of each scale domain, in the order of their Morton codes, packed into consecutive
//! shards up to a budget of estimated D1 bytes, then the objects that are not placed, in
//! [`LOOSE_BUCKETS`] buckets by their ids, packed after them in the same way. A cell or a bucket
//! that alone passes the budget is refused, naming it.

use std::collections::BTreeMap;
use std::fmt;

use univ_model::release::Cell;
use univ_model::release::shards::morton;

use super::kept::Kept;
use super::placing::place;

/// The buckets the objects not placed are divided into: object `id` is in bucket
/// `id mod LOOSE_BUCKETS`, and its shard is its bucket's.
pub const LOOSE_BUCKETS: u64 = 64;

/// The objects of one level-0 cell and their estimated D1 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellLoad {
    /// The scale domain's place in the packing order, and its name.
    pub domain: (usize, &'static str),
    pub cell: Cell,
    pub objects: u64,
    pub bytes: u64,
}

/// A run of cells of one domain that a shard holds, by their Morton codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub domain: &'static str,
    pub first: u64,
    pub last: u64,
    pub cells: u64,
}

/// One object shard as packed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packed {
    pub runs: Vec<Run>,
    pub objects: u64,
    /// The objects not placed it holds.
    pub unplaced: u64,
    pub bytes: u64,
}

/// The shards of a release: what each holds, and the shard of each cell and bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packing {
    pub shards: Vec<Packed>,
    /// The shard of each cell, in the order the cells were given.
    pub(super) cells: Vec<u32>,
    /// The shard of each bucket of objects not placed that has one.
    pub(super) loose: Vec<Option<u32>>,
    /// The Morton codes of the cells each shard holds, by domain.
    pub(super) held: Vec<BTreeMap<String, Vec<u64>>>,
}

impl Packing {
    /// The number of shards.
    #[must_use]
    pub fn count(&self) -> u32 {
        u32::try_from(self.shards.len()).unwrap_or(u32::MAX)
    }

    /// The shard of the cell given at `cell` among the cells packed.
    #[must_use]
    pub fn shard_of_cell(&self, cell: usize) -> u32 {
        self.cells[cell]
    }

    /// The shard of the object `id`, which is not placed: its bucket's.
    #[must_use]
    pub fn shard_of_unplaced(&self, id: u64) -> u32 {
        self.loose[bucket(id)].unwrap_or(0)
    }

    /// The Morton codes of the cells each shard holds, shard by shard and domain by domain, for
    /// the next release to keep.
    #[must_use]
    pub fn held(&self) -> Vec<BTreeMap<String, Vec<u64>>> {
        self.held.clone()
    }

    /// The shard of each bucket of objects not placed that has one, by bucket: each that holds
    /// any, and each that a shard held before though it holds none now.
    #[must_use]
    pub fn buckets(&self) -> BTreeMap<u64, u32> {
        (0_u64..)
            .zip(&self.loose)
            .filter_map(|(k, shard)| Some((k, (*shard)?)))
            .collect()
    }
}

/// The bucket of the object not placed `id`.
pub(super) fn bucket(id: u64) -> usize {
    usize::try_from(id % LOOSE_BUCKETS).unwrap_or(0)
}

/// Why the objects cannot be packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    /// The objects of one cell alone pass the budget.
    OversizedCell { load: CellLoad, budget: u64 },
    /// The objects not placed of one bucket alone pass the budget.
    OversizedBucket {
        bucket: u64,
        objects: u64,
        bytes: u64,
        budget: u64,
    },
    /// A cell lies beyond the reach of a Morton code.
    BeyondReach { domain: &'static str, cell: Cell },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OversizedCell { load, budget } => write!(
                f,
                "cell ({}, {}, {}) of {} holds {} objects, estimated at {} bytes, more than an \
                 object shard's budget of {budget} bytes",
                load.cell.ix, load.cell.iy, load.cell.iz, load.domain.1, load.objects, load.bytes
            ),
            Self::OversizedBucket {
                bucket,
                objects,
                bytes,
                budget,
            } => write!(
                f,
                "the {objects} objects not placed whose id is {bucket} modulo {LOOSE_BUCKETS}, \
                 estimated at {bytes} bytes, are more than an object shard's budget of {budget} \
                 bytes"
            ),
            Self::BeyondReach { domain, cell } => write!(
                f,
                "cell ({}, {}, {}) of {domain} lies beyond the reach of the shards' cell order",
                cell.ix, cell.iy, cell.iz
            ),
        }
    }
}

/// Packs `cells` and the objects not placed, `loose` as (id, estimated bytes), into object
/// shards of at most `budget` estimated bytes each: always at least one shard.
///
/// A cell or bucket that a shard of `previous`, the release before, held stays in it, so that
/// its objects' rows stay in the same database; new cells and buckets fill the last shard and
/// new ones (see [`super::placing`]).
///
/// # Errors
///
/// Fails when a cell or a bucket of objects not placed alone passes the budget, or a cell lies
/// beyond a Morton code's reach.
pub fn pack(
    cells: &[CellLoad],
    loose: &[(u64, u64)],
    budget: u64,
    previous: &Kept,
) -> Result<Packing, PackError> {
    let order = ordered(cells, budget)?;
    let buckets = bucketed(loose, budget)?;
    Ok(place(cells, &order, &buckets, budget, previous))
}

/// A cell in packing order: its (domain's place, Morton code), then its place among the cells.
pub(super) type Ordered = ((usize, u64), usize);

/// The cells in packing order, by domain and Morton code.
fn ordered(cells: &[CellLoad], budget: u64) -> Result<Vec<Ordered>, PackError> {
    let mut order = Vec::with_capacity(cells.len());
    for (i, load) in cells.iter().enumerate() {
        let code = morton(load.cell).ok_or(PackError::BeyondReach {
            domain: load.domain.1,
            cell: load.cell,
        })?;
        if load.bytes > budget {
            return Err(PackError::OversizedCell {
                load: *load,
                budget,
            });
        }
        order.push(((load.domain.0, code), i));
    }
    order.sort_unstable_by_key(|(key, _)| *key);
    Ok(order)
}

/// The objects not placed and their bytes in each bucket.
fn bucketed(loose: &[(u64, u64)], budget: u64) -> Result<Vec<(u64, u64)>, PackError> {
    let mut buckets = vec![(0_u64, 0_u64); bucket(LOOSE_BUCKETS - 1) + 1];
    for &(id, bytes) in loose {
        let b = &mut buckets[bucket(id)];
        b.0 += 1;
        b.1 += bytes;
    }
    if let Some((k, &(objects, bytes))) = (0..).zip(&buckets).find(|(_, b)| b.1 > budget) {
        return Err(PackError::OversizedBucket {
            bucket: k,
            objects,
            bytes,
            budget,
        });
    }
    Ok(buckets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ix: i64, iy: i64, iz: i64) -> Cell {
        Cell { ix, iy, iz }
    }

    /// What `p` keeps for the next packing.
    fn kept(p: &Packing) -> Kept {
        Kept {
            cells: p.held(),
            loose: p.buckets(),
            ..Kept::default()
        }
    }

    fn load(domain: usize, at: Cell, bytes: u64) -> CellLoad {
        let name = ["stars", "nearby_galaxies", "cosmic_web"][domain];
        CellLoad {
            domain: (domain, name),
            cell: at,
            objects: bytes / 10,
            bytes,
        }
    }

    #[test]
    fn cells_fill_shards_in_morton_order_domain_by_domain() {
        // Given out of order: the packing sorts by domain, then Morton code.
        let cells = [
            load(2, cell(1, 1, 1), 400),
            load(0, cell(0, 0, 0), 300),
            load(2, cell(0, 0, 0), 300),
            load(0, cell(1, 0, 0), 300),
            load(2, cell(1, 0, 0), 300),
        ];
        let p = pack(&cells, &[], 700, &Kept::default()).unwrap();
        let bytes: Vec<u64> = p.shards.iter().map(|s| s.bytes).collect();
        assert_eq!(bytes, [600, 600, 400]);
        // Shard 1 begins in the last domain and holds a run of two of its cells.
        let run = &p.shards[1].runs[0];
        assert_eq!((run.domain, run.cells), ("cosmic_web", 2));
        assert_eq!(run.first, morton(cell(0, 0, 0)).unwrap());
        let shards: Vec<u32> = (0..cells.len()).map(|i| p.shard_of_cell(i)).collect();
        assert_eq!(shards, [2, 0, 1, 0, 1]);
    }

    #[test]
    fn objects_not_placed_go_by_their_ids_bucket_after_the_cells() {
        let cells = [load(2, cell(0, 0, 0), 500), load(2, cell(1, 0, 0), 500)];
        let loose = [(1, 100), (2, 100), (65, 100)];
        let p = pack(&cells, &loose, 1_000, &Kept::default()).unwrap();
        // The cells fill shard 0; the buckets of ids 1 and 65 (one bucket), and 2, take shard 1.
        assert_eq!(p.count(), 2);
        assert!(p.shards.iter().all(|s| s.bytes <= 1_000));
        assert_eq!((p.shards[1].objects, p.shards[1].unplaced), (3, 3));
        assert_eq!(p.shard_of_unplaced(1), p.shard_of_unplaced(65));
        assert_eq!(p.shard_of_unplaced(2), 1);
        // With no cell at all there is still one shard.
        assert_eq!(
            pack(&[], &loose, 1_000, &Kept::default()).unwrap().count(),
            1
        );
        assert_eq!(pack(&[], &[], 1_000, &Kept::default()).unwrap().count(), 1);
    }

    #[test]
    fn a_cell_or_bucket_past_the_budget_is_refused_by_name() {
        let big = load(2, cell(-3, 4, 5), 2_000);
        let err = pack(
            &[load(2, cell(0, 0, 0), 10), big],
            &[],
            1_000,
            &Kept::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            PackError::OversizedCell {
                load: big,
                budget: 1_000
            }
        );
        assert_eq!(
            err.to_string(),
            "cell (-3, 4, 5) of cosmic_web holds 200 objects, estimated at 2000 bytes, more than \
             an object shard's budget of 1000 bytes"
        );
        let err = pack(&[], &[(3, 600), (67, 600)], 1_000, &Kept::default()).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("the 2 objects not placed whose id is 3 modulo 64"),
            "{err}"
        );
    }

    #[test]
    fn cells_and_buckets_stay_in_the_shards_that_held_them() {
        let cells = [
            load(2, cell(0, 0, 0), 300),
            load(2, cell(1, 0, 0), 300),
            load(2, cell(0, 1, 0), 300),
        ];
        let loose = [(1, 100)];
        let first = pack(&cells, &loose, 700, &Kept::default()).unwrap();
        let previous = kept(&first);
        assert_eq!(
            previous.loose.len(),
            1,
            "only a bucket that held objects has a shard"
        );
        // The next release adds a cell before the others in Morton order and one more object
        // not placed in another bucket: what was held stays, the new go after it.
        let next_cells = [load(2, cell(-1, 0, 0), 100), cells[0], cells[1], cells[2]];
        let next_loose = [(1, 100), (2, 50)];
        let next = pack(&next_cells, &next_loose, 700, &previous).unwrap();
        for (i, c) in (1..).zip(0..cells.len()) {
            assert_eq!(next.shard_of_cell(i), first.shard_of_cell(c));
        }
        assert_eq!(next.shard_of_unplaced(1), first.shard_of_unplaced(1));
        assert_eq!(next.count(), first.count());
        assert!(next.shards.iter().all(|s| s.bytes <= 700));
        // Packed again with nothing new, the shards keep the same runs.
        let again = pack(&cells, &loose, 700, &previous).unwrap();
        assert_eq!(kept(&again), previous);
    }

    #[test]
    fn a_shard_that_outgrows_its_budget_gives_up_its_last_cells() {
        let cells = [
            load(2, cell(0, 0, 0), 200),
            load(2, cell(1, 0, 0), 200),
            load(2, cell(0, 1, 0), 200),
        ];
        let first = pack(&cells, &[], 700, &Kept::default()).unwrap();
        assert_eq!(first.count(), 1);
        let grown = [cells[0], cells[1], load(2, cell(0, 1, 0), 400)];
        let next = pack(&grown, &[], 700, &kept(&first)).unwrap();
        // The last cell in Morton order moves to a new shard; the first two stay.
        let at: Vec<u32> = (0..3).map(|i| next.shard_of_cell(i)).collect();
        assert_eq!(at, [0, 0, 1]);
        assert!(next.shards.iter().all(|s| s.bytes <= 700));
    }

    #[test]
    fn a_new_cell_among_a_shards_cells_never_pushes_them_out() {
        // Shard 0 holds cells at Morton codes 0 and 3 (one run), shard 1 the cell after them.
        let cells = [
            load(2, cell(0, 0, 0), 300),
            load(2, cell(1, 1, 0), 300),
            load(2, cell(0, 0, 1), 300),
        ];
        let first = pack(&cells, &[], 600, &Kept::default()).unwrap();
        let held: Vec<u32> = (0..3).map(|i| first.shard_of_cell(i)).collect();
        assert_eq!(held, [0, 0, 1]);
        // A new cell lies inside shard 0's run: it goes to the last shard, not into shard 0,
        // which keeps both its cells.
        let next_cells = [cells[0], cells[1], cells[2], load(2, cell(1, 0, 0), 200)];
        let next = pack(&next_cells, &[], 600, &kept(&first)).unwrap();
        let at: Vec<u32> = (0..4).map(|i| next.shard_of_cell(i)).collect();
        assert_eq!(at, [0, 0, 1, 1]);
        // Shard 1 now holds two runs of one domain, either side of shard 0's last cell.
        assert_eq!(next.shards[1].runs.len(), 2);
    }

    #[test]
    fn an_emptied_bucket_keeps_its_shard_and_an_emptied_last_shard_goes() {
        let cells = [load(2, cell(0, 0, 0), 500), load(2, cell(1, 0, 0), 500)];
        let first = pack(&cells, &[], 600, &Kept::default()).unwrap();
        assert_eq!(first.count(), 2);
        // The second cell is gone: the last shard, now empty, is dropped.
        let next = pack(&cells[..1], &[], 600, &kept(&first)).unwrap();
        assert_eq!(next.count(), 1);
        // Shard 0 held bucket 5; with no object in it now, the bucket keeps its shard all the
        // same, and its objects return to it.
        let previous = Kept {
            loose: BTreeMap::from([(5, 0)]),
            ..kept(&next)
        };
        let empty = pack(&cells[..1], &[], 600, &previous).unwrap();
        assert_eq!(empty.buckets().get(&5), Some(&0));
        let back = pack(&cells[..1], &[(5, 50)], 600, &kept(&empty)).unwrap();
        assert_eq!(back.shard_of_unplaced(5), 0);
    }
}
