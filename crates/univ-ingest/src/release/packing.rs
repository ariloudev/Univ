//! A release's objects divided among object shards (`univ_model::release::shards`): the level-0
//! cells of each scale domain, in the order of their Morton codes, packed into consecutive
//! shards up to a budget of estimated D1 bytes, and each object that is not placed given the
//! shard of its id.
//!
//! The objects not placed are spread over the shards the cells fill. Where they would take a
//! shard past the budget, the cells are packed again with the budget less the largest share of
//! them a shard took, until every shard fits; a cell that alone passes the budget is refused,
//! naming it.

use std::fmt;

use univ_model::release::Cell;
use univ_model::release::shards::{morton, unplaced};

/// A cell's place in the packing order: its domain's place, then its Morton code.
type Key = (usize, u64);

/// The objects of one level-0 cell and their estimated D1 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub unplaced: u64,
    pub bytes: u64,
}

/// The shards of a release: what each holds, and the shard of each cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packing {
    pub shards: Vec<Packed>,
    /// Each cell's (domain, Morton code) and shard, sorted.
    cells: Vec<(Key, u32)>,
}

impl Packing {
    /// The number of shards.
    #[must_use]
    pub fn count(&self) -> u32 {
        u32::try_from(self.shards.len()).unwrap_or(u32::MAX)
    }

    /// The shard of the cell `cell` of the domain at `domain` in the packing order: none for a
    /// cell the packing was not given.
    #[must_use]
    pub fn shard_of_cell(&self, domain: usize, cell: Cell) -> Option<u32> {
        let key = (domain, morton(cell)?);
        let i = self.cells.binary_search_by_key(&key, |(k, _)| *k).ok()?;
        Some(self.cells[i].1)
    }

    /// The shard of the object `id`, which is not placed.
    #[must_use]
    pub fn shard_of_unplaced(&self, id: u64) -> u32 {
        unplaced(id, self.count())
    }
}

/// Why the objects cannot be packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    /// The objects of one cell alone pass the budget.
    OversizedCell { load: CellLoad, budget: u64 },
    /// A cell lies beyond the reach of a Morton code.
    BeyondReach { domain: &'static str, cell: Cell },
    /// The objects not placed cannot be spread within the budget.
    Unplaced { bytes: u64, budget: u64 },
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
            Self::BeyondReach { domain, cell } => write!(
                f,
                "cell ({}, {}, {}) of {domain} lies beyond the reach of the shards' cell order",
                cell.ix, cell.iy, cell.iz
            ),
            Self::Unplaced { bytes, budget } => write!(
                f,
                "the objects not placed, estimated at {bytes} bytes, cannot be spread over the \
                 object shards within their budget of {budget} bytes"
            ),
        }
    }
}

/// Packs `cells` and the objects not placed, `loose` as (id, estimated bytes), into object
/// shards of at most `budget` estimated bytes each: always at least one shard.
///
/// # Errors
///
/// Fails when a cell alone passes the budget or lies beyond a Morton code's reach, or the
/// objects not placed cannot be spread within it.
pub fn pack(cells: &[CellLoad], loose: &[(u64, u64)], budget: u64) -> Result<Packing, PackError> {
    let mut ordered = Vec::with_capacity(cells.len());
    for load in cells {
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
        ordered.push(((load.domain.0, code), load));
    }
    ordered.sort_by_key(|(key, _)| *key);
    let mut reserve = 0;
    loop {
        let mut packing = fill(&ordered, budget - reserve);
        let count = packing.count();
        let mut shares = vec![0_u64; packing.shards.len()];
        for &(id, bytes) in loose {
            let i = unplaced(id, count) as usize;
            let shard = &mut packing.shards[i];
            shard.objects += 1;
            shard.unplaced += 1;
            shard.bytes += bytes;
            shares[i] += bytes;
        }
        if packing.shards.iter().all(|s| s.bytes <= budget) {
            return Ok(packing);
        }
        let share = shares.into_iter().max().unwrap_or(0);
        if share <= reserve || share >= budget {
            return Err(PackError::Unplaced {
                bytes: loose.iter().map(|(_, b)| b).sum(),
                budget,
            });
        }
        reserve = share;
    }
}

/// The cells, ordered, packed greedily into shards of at most `limit` bytes; a cell larger than
/// the limit (but within the budget) takes a shard of its own.
fn fill(ordered: &[(Key, &CellLoad)], limit: u64) -> Packing {
    let mut shards = vec![Packed::default()];
    let mut owners = Vec::with_capacity(ordered.len());
    for &(key, load) in ordered {
        let last = shards.len() - 1;
        if shards[last].bytes > 0 && shards[last].bytes + load.bytes > limit {
            shards.push(Packed::default());
        }
        let index = shards.len() - 1;
        let shard = &mut shards[index];
        match shard.runs.last_mut() {
            Some(run) if run.domain == load.domain.1 => {
                run.last = key.1;
                run.cells += 1;
            }
            _ => shard.runs.push(Run {
                domain: load.domain.1,
                first: key.1,
                last: key.1,
                cells: 1,
            }),
        }
        shard.objects += load.objects;
        shard.bytes += load.bytes;
        owners.push((key, u32::try_from(index).unwrap_or(u32::MAX)));
    }
    Packing {
        shards,
        cells: owners,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(domain: usize, ix: i64, iy: i64, iz: i64, bytes: u64) -> CellLoad {
        let name = ["stars", "nearby_galaxies", "cosmic_web"][domain];
        CellLoad {
            domain: (domain, name),
            cell: Cell { ix, iy, iz },
            objects: bytes / 10,
            bytes,
        }
    }

    #[test]
    fn cells_fill_shards_in_morton_order_domain_by_domain() {
        // Given out of order: the packing sorts by domain, then Morton code.
        let cells = [
            load(2, 1, 1, 1, 400),
            load(0, 0, 0, 0, 300),
            load(2, 0, 0, 0, 300),
            load(0, 1, 0, 0, 300),
            load(2, 1, 0, 0, 300),
        ];
        let p = pack(&cells, &[], 700).unwrap();
        let bytes: Vec<u64> = p.shards.iter().map(|s| s.bytes).collect();
        assert_eq!(bytes, [600, 600, 400]);
        // Shard 1 begins in the last domain and holds a run of two of its cells.
        let run = &p.shards[1].runs[0];
        assert_eq!((run.domain, run.cells), ("cosmic_web", 2));
        assert_eq!(
            run.first,
            morton(Cell {
                ix: 0,
                iy: 0,
                iz: 0
            })
            .unwrap()
        );
        assert_eq!(
            p.shard_of_cell(
                2,
                Cell {
                    ix: 1,
                    iy: 0,
                    iz: 0
                }
            ),
            Some(1)
        );
        assert_eq!(
            p.shard_of_cell(
                2,
                Cell {
                    ix: 1,
                    iy: 1,
                    iz: 1
                }
            ),
            Some(2)
        );
        assert_eq!(
            p.shard_of_cell(
                0,
                Cell {
                    ix: 1,
                    iy: 0,
                    iz: 0
                }
            ),
            Some(0)
        );
        assert_eq!(
            p.shard_of_cell(
                1,
                Cell {
                    ix: 0,
                    iy: 0,
                    iz: 0
                }
            ),
            None
        );
    }

    #[test]
    fn objects_not_placed_go_by_id_and_the_cells_make_room_for_them() {
        let cells = [load(2, 0, 0, 0, 500), load(2, 1, 0, 0, 500)];
        // Packed at 1,000 the two cells fill one shard, which the loose objects then overfill.
        let loose = [(1, 100), (2, 100), (3, 100)];
        let p = pack(&cells, &loose, 1_000).unwrap();
        assert_eq!(p.count(), 2);
        assert!(p.shards.iter().all(|s| s.bytes <= 1_000));
        assert_eq!(p.shards.iter().map(|s| s.unplaced).sum::<u64>(), 3);
        assert_eq!(p.shard_of_unplaced(3), 1);
        assert_eq!(p.shard_of_unplaced(4), 0);
        // With no cell at all there is still one shard.
        assert_eq!(pack(&[], &loose, 1_000).unwrap().count(), 1);
    }

    #[test]
    fn a_cell_past_the_budget_is_refused_by_name() {
        let big = load(2, -3, 4, 5, 2_000);
        let err = pack(&[load(2, 0, 0, 0, 10), big], &[], 1_000).unwrap_err();
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
    }
}
