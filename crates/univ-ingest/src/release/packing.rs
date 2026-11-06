//! A release's objects divided among object shards (`univ_model::release::shards`): the level-0
//! cells of each scale domain, in the order of their Morton codes, packed into consecutive
//! shards up to a budget of estimated D1 bytes, then the objects that are not placed, in
//! [`LOOSE_BUCKETS`] buckets by their ids, packed after them in the same way. A cell or a bucket
//! that alone passes the budget is refused, naming it.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use univ_model::release::Cell;
use univ_model::release::shards::morton;

use super::kept::KeptRun;

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
    /// The shard of each bucket of objects not placed that holds any.
    loose: Vec<Option<u32>>,
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

    /// The runs of cells each shard holds and the shard of each bucket that holds objects, for
    /// the next release to keep.
    #[must_use]
    pub fn kept(&self) -> (Vec<Vec<KeptRun>>, BTreeMap<u64, u32>) {
        let shards = self
            .shards
            .iter()
            .map(|s| {
                s.runs
                    .iter()
                    .map(|r| KeptRun {
                        domain: r.domain.to_owned(),
                        first: r.first,
                        last: r.last,
                    })
                    .collect()
            })
            .collect();
        let loose = (0_u64..)
            .zip(&self.loose)
            .filter_map(|(k, shard)| Some((k, (*shard)?)))
            .collect();
        (shards, loose)
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

/// The object shards of the release before: the runs of cells each held and the shard of each
/// bucket of objects not placed. Empty before the first release.
#[derive(Clone, Copy, Debug)]
pub struct Previous<'a> {
    pub shards: &'a [Vec<KeptRun>],
    pub loose: &'a BTreeMap<u64, u32>,
}

/// No bucket held by any shard.
static NO_BUCKETS: BTreeMap<u64, u32> = BTreeMap::new();

impl Default for Previous<'_> {
    fn default() -> Self {
        Self {
            shards: &[],
            loose: &NO_BUCKETS,
        }
    }
}

/// Packs `cells` and the objects not placed, `loose` as (id, estimated bytes), into object
/// shards of at most `budget` estimated bytes each: always at least one shard.
///
/// A cell or bucket that a shard of `previous` held stays in it, so that its objects' rows stay
/// in the same database; the rest fill the last shard and new ones. A shard that its kept cells
/// and buckets would take past the budget gives up its last ones, in packing order, to them.
///
/// # Errors
///
/// Fails when a cell or a bucket of objects not placed alone passes the budget, or a cell lies
/// beyond a Morton code's reach.
pub fn pack(
    cells: &[CellLoad],
    loose: &[(u64, u64)],
    budget: u64,
    previous: Previous<'_>,
) -> Result<Packing, PackError> {
    let order = ordered(cells, budget)?;
    let buckets = bucketed(loose, budget)?;
    let owners = Owners::new(previous.shards);
    Ok(place(
        cells,
        &order,
        &buckets,
        budget,
        &|load: &CellLoad, code| owners.of(load.domain.1, code),
        &|k| previous.loose.get(&k).copied(),
        previous.shards.len(),
    ))
}

/// A cell in packing order: its (domain's place, Morton code), then its place among the cells.
type Ordered = ((usize, u64), usize);

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

/// The shard of the release before that held each cell, from its runs.
struct Owners<'a> {
    /// Each domain's runs as (first, last, shard), by their first code.
    runs: HashMap<&'a str, Vec<(u64, u64, u32)>>,
}

impl<'a> Owners<'a> {
    fn new(shards: &'a [Vec<KeptRun>]) -> Self {
        let mut runs: HashMap<&str, Vec<(u64, u64, u32)>> = HashMap::new();
        for (shard, held) in (0_u32..).zip(shards) {
            for run in held {
                runs.entry(run.domain.as_str())
                    .or_default()
                    .push((run.first, run.last, shard));
            }
        }
        for each in runs.values_mut() {
            each.sort_unstable();
        }
        Self { runs }
    }

    /// The shard whose run of `domain` holds the cell of Morton code `code`.
    fn of(&self, domain: &str, code: u64) -> Option<u32> {
        let runs = self.runs.get(domain)?;
        let after = runs.partition_point(|r| r.0 <= code);
        let &(_, last, shard) = runs.get(after.checked_sub(1)?)?;
        (code <= last).then_some(shard)
    }
}

/// Packs the cells, given in `order`, and the buckets: each that `cell_owner` or `bucket_owner`
/// names into that shard, and the rest, in order, into the last shard and new ones, starting
/// from `shards` shards. Each shard's runs follow the cells in order, one run for each stretch
/// of consecutive cells of one domain that it holds.
fn place(
    cells: &[CellLoad],
    order: &[Ordered],
    buckets: &[(u64, u64)],
    budget: u64,
    cell_owner: &dyn Fn(&CellLoad, u64) -> Option<u32>,
    bucket_owner: &dyn Fn(u64) -> Option<u32>,
    shards: usize,
) -> Packing {
    let mut fill = Fill {
        shards: vec![Packed::default(); shards.max(1)],
        budget,
    };
    let mut shard_of_cell: Vec<Option<u32>> = vec![None; cells.len()];
    for &((_, code), i) in order {
        let load = &cells[i];
        if let Some(owner) = cell_owner(load, code).filter(|&o| (o as usize) < fill.shards.len()) {
            fill.take(owner, load.objects, load.bytes, 0);
            shard_of_cell[i] = Some(owner);
        }
    }
    let mut shard_of_bucket: Vec<Option<u32>> = vec![None; buckets.len()];
    for (k, &(objects, bytes)) in (0_u64..).zip(buckets) {
        let owner = bucket_owner(k).filter(|&o| (o as usize) < fill.shards.len());
        if let (Some(owner), true) = (owner, objects > 0) {
            fill.take(owner, objects, bytes, objects);
            shard_of_bucket[bucket(k)] = Some(owner);
        }
    }
    // A shard past the budget gives up its buckets, then its cells, last first.
    for (at, &(objects, bytes)) in buckets.iter().enumerate().rev() {
        if let Some(owner) = shard_of_bucket[at].filter(|&o| fill.over(o)) {
            fill.take_back(owner, objects, bytes, objects);
            shard_of_bucket[at] = None;
        }
    }
    for &(_, i) in order.iter().rev() {
        if let Some(owner) = shard_of_cell[i].filter(|&o| fill.over(o)) {
            fill.take_back(owner, cells[i].objects, cells[i].bytes, 0);
            shard_of_cell[i] = None;
        }
    }
    for &(_, i) in order {
        if shard_of_cell[i].is_none() {
            let load = &cells[i];
            let index = fill.room(load.bytes);
            fill.take(index, load.objects, load.bytes, 0);
            shard_of_cell[i] = Some(index);
        }
    }
    for (k, &(objects, bytes)) in (0_u64..).zip(buckets) {
        if objects > 0 && shard_of_bucket[bucket(k)].is_none() {
            let index = fill.room(bytes);
            fill.take(index, objects, bytes, objects);
            shard_of_bucket[bucket(k)] = Some(index);
        }
    }
    let cells_shards: Vec<u32> = shard_of_cell.into_iter().map(|s| s.unwrap_or(0)).collect();
    let mut last: Option<u32> = None;
    for &((_, code), i) in order {
        let index = cells_shards[i];
        let domain = cells[i].domain.1;
        let runs = &mut fill.shards[index as usize].runs;
        match runs.last_mut() {
            Some(run) if last == Some(index) && run.domain == domain => {
                run.last = code;
                run.cells += 1;
            }
            _ => runs.push(Run {
                domain,
                first: code,
                last: code,
                cells: 1,
            }),
        }
        last = Some(index);
    }
    Packing {
        shards: fill.shards,
        cells: cells_shards,
        loose: shard_of_bucket,
    }
}

/// Shards filled one after another up to the budget.
struct Fill {
    shards: Vec<Packed>,
    budget: u64,
}

impl Fill {
    /// The number of the shard that takes `bytes` more: the last, or a new one when they would
    /// take the last past the budget.
    fn room(&mut self, bytes: u64) -> u32 {
        let full = self
            .shards
            .last()
            .is_some_and(|s| s.bytes > 0 && s.bytes + bytes > self.budget);
        if full {
            self.shards.push(Packed::default());
        }
        u32::try_from(self.shards.len() - 1).unwrap_or(u32::MAX)
    }

    /// Counts `objects` (of them `unplaced` not placed) and `bytes` into shard `index`.
    fn take(&mut self, index: u32, objects: u64, bytes: u64, unplaced: u64) {
        if let Some(shard) = self.shards.get_mut(index as usize) {
            shard.objects += objects;
            shard.unplaced += unplaced;
            shard.bytes += bytes;
        }
    }

    /// Counts what [`Self::take`] counted into shard `index` out of it again.
    fn take_back(&mut self, index: u32, objects: u64, bytes: u64, unplaced: u64) {
        if let Some(shard) = self.shards.get_mut(index as usize) {
            shard.objects -= objects;
            shard.unplaced -= unplaced;
            shard.bytes -= bytes;
        }
    }

    /// Whether shard `index` holds more than the budget.
    fn over(&self, index: u32) -> bool {
        self.shards
            .get(index as usize)
            .is_some_and(|s| s.bytes > self.budget)
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
        let p = pack(&cells, &[], 700, Previous::default()).unwrap();
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
        let p = pack(&cells, &loose, 1_000, Previous::default()).unwrap();
        // The cells fill shard 0; the buckets of ids 1 and 65 (one bucket), and 2, take shard 1.
        assert_eq!(p.count(), 2);
        assert!(p.shards.iter().all(|s| s.bytes <= 1_000));
        assert_eq!((p.shards[1].objects, p.shards[1].unplaced), (3, 3));
        assert_eq!(p.shard_of_unplaced(1), p.shard_of_unplaced(65));
        assert_eq!(p.shard_of_unplaced(2), 1);
        // With no cell at all there is still one shard.
        assert_eq!(
            pack(&[], &loose, 1_000, Previous::default())
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            pack(&[], &[], 1_000, Previous::default()).unwrap().count(),
            1
        );
    }

    #[test]
    fn a_cell_or_bucket_past_the_budget_is_refused_by_name() {
        let big = load(2, cell(-3, 4, 5), 2_000);
        let err = pack(
            &[load(2, cell(0, 0, 0), 10), big],
            &[],
            1_000,
            Previous::default(),
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
        let err = pack(&[], &[(3, 600), (67, 600)], 1_000, Previous::default()).unwrap_err();
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
        let first = pack(&cells, &loose, 700, Previous::default()).unwrap();
        let (shards, buckets) = first.kept();
        assert_eq!(
            buckets.len(),
            1,
            "only the bucket that holds an object is kept"
        );
        // The next release adds a cell before the others in Morton order and one more object
        // not placed in another bucket: what was held stays, the new go after it.
        let next_cells = [load(2, cell(-1, 0, 0), 100), cells[0], cells[1], cells[2]];
        let next_loose = [(1, 100), (2, 50)];
        let previous = Previous {
            shards: &shards,
            loose: &buckets,
        };
        let next = pack(&next_cells, &next_loose, 700, previous).unwrap();
        for (i, c) in (1..).zip(0..cells.len()) {
            assert_eq!(next.shard_of_cell(i), first.shard_of_cell(c));
        }
        assert_eq!(next.shard_of_unplaced(1), first.shard_of_unplaced(1));
        assert_eq!(next.count(), first.count());
        assert!(next.shards.iter().all(|s| s.bytes <= 700));
        // Packed again with nothing new, the shards keep the same runs.
        let again = pack(&cells, &loose, 700, previous).unwrap();
        assert_eq!(again.kept(), (shards.clone(), buckets.clone()));
    }

    #[test]
    fn a_shard_that_outgrows_its_budget_gives_up_its_last_cells() {
        let cells = [
            load(2, cell(0, 0, 0), 200),
            load(2, cell(1, 0, 0), 200),
            load(2, cell(0, 1, 0), 200),
        ];
        let first = pack(&cells, &[], 700, Previous::default()).unwrap();
        assert_eq!(first.count(), 1);
        let (shards, buckets) = first.kept();
        let grown = [cells[0], cells[1], load(2, cell(0, 1, 0), 400)];
        let previous = Previous {
            shards: &shards,
            loose: &buckets,
        };
        let next = pack(&grown, &[], 700, previous).unwrap();
        // The last cell in Morton order moves to a new shard; the first two stay.
        let at: Vec<u32> = (0..3).map(|i| next.shard_of_cell(i)).collect();
        assert_eq!(at, [0, 0, 1]);
        assert!(next.shards.iter().all(|s| s.bytes <= 700));
    }
}
