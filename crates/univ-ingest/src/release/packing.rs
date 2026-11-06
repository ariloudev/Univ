//! A release's objects divided among object shards (`univ_model::release::shards`): the level-0
//! cells of each scale domain, in the order of their Morton codes, packed into consecutive
//! shards up to a budget of estimated D1 bytes, then the objects that are not placed, in
//! [`LOOSE_BUCKETS`] buckets by their ids, packed after them in the same way. A cell or a bucket
//! that alone passes the budget is refused, naming it.

use std::fmt;

use univ_model::release::Cell;
use univ_model::release::shards::morton;

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
    cells: Vec<u32>,
    /// The shard of each bucket of objects not placed.
    loose: Vec<u32>,
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
        self.loose[bucket(id)]
    }
}

/// The bucket of the object not placed `id`.
fn bucket(id: u64) -> usize {
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
/// # Errors
///
/// Fails when a cell or a bucket of objects not placed alone passes the budget, or a cell lies
/// beyond a Morton code's reach.
pub fn pack(cells: &[CellLoad], loose: &[(u64, u64)], budget: u64) -> Result<Packing, PackError> {
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
    let mut fill = Fill {
        shards: vec![Packed::default()],
        budget,
    };
    let mut shard_of_cell = vec![0; cells.len()];
    for ((_, code), i) in order {
        let load = &cells[i];
        let (index, shard) = fill.room(load.bytes);
        match shard.runs.last_mut() {
            Some(run) if run.domain == load.domain.1 => {
                run.last = code;
                run.cells += 1;
            }
            _ => shard.runs.push(Run {
                domain: load.domain.1,
                first: code,
                last: code,
                cells: 1,
            }),
        }
        shard.objects += load.objects;
        shard.bytes += load.bytes;
        shard_of_cell[i] = index;
    }
    let mut shard_of_bucket = vec![0; buckets.len()];
    for (k, (objects, bytes)) in buckets.into_iter().enumerate() {
        if objects == 0 {
            continue;
        }
        let (index, shard) = fill.room(bytes);
        shard.objects += objects;
        shard.unplaced += objects;
        shard.bytes += bytes;
        shard_of_bucket[k] = index;
    }
    Ok(Packing {
        shards: fill.shards,
        cells: shard_of_cell,
        loose: shard_of_bucket,
    })
}

/// Shards filled one after another up to the budget.
struct Fill {
    shards: Vec<Packed>,
    budget: u64,
}

impl Fill {
    /// The shard, and its number, that takes `bytes` more: the last, or a new one when they
    /// would take the last past the budget.
    fn room(&mut self, bytes: u64) -> (u32, &mut Packed) {
        let full = self
            .shards
            .last()
            .is_some_and(|s| s.bytes > 0 && s.bytes + bytes > self.budget);
        if full {
            self.shards.push(Packed::default());
        }
        let index = u32::try_from(self.shards.len() - 1).unwrap_or(u32::MAX);
        let last = self.shards.len() - 1;
        (index, &mut self.shards[last])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ix: i64, iy: i64, iz: i64) -> Cell {
        Cell { ix, iy, iz }
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
        let p = pack(&cells, &[], 700).unwrap();
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
        let p = pack(&cells, &loose, 1_000).unwrap();
        // The cells fill shard 0; the buckets of ids 1 and 65 (one bucket), and 2, take shard 1.
        assert_eq!(p.count(), 2);
        assert!(p.shards.iter().all(|s| s.bytes <= 1_000));
        assert_eq!((p.shards[1].objects, p.shards[1].unplaced), (3, 3));
        assert_eq!(p.shard_of_unplaced(1), p.shard_of_unplaced(65));
        assert_eq!(p.shard_of_unplaced(2), 1);
        // With no cell at all there is still one shard.
        assert_eq!(pack(&[], &loose, 1_000).unwrap().count(), 1);
        assert_eq!(pack(&[], &[], 1_000).unwrap().count(), 1);
    }

    #[test]
    fn a_cell_or_bucket_past_the_budget_is_refused_by_name() {
        let big = load(2, cell(-3, 4, 5), 2_000);
        let err = pack(&[load(2, cell(0, 0, 0), 10), big], &[], 1_000).unwrap_err();
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
        let err = pack(&[], &[(3, 600), (67, 600)], 1_000).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("the 2 objects not placed whose id is 3 modulo 64"),
            "{err}"
        );
    }
}
