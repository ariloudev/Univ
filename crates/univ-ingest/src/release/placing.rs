//! Where [`super::packing::pack`] puts each cell and bucket of objects not placed: in the shard
//! of the release before that held it, so that its objects' rows stay in the same database, and
//! a new one, in order, in the last shard or a new one. A shard that what it held would take
//! past the budget gives up its buckets, then its cells, last first: only a shard whose own
//! cells grew gives any up.

use std::collections::{BTreeMap, HashMap};

use super::kept::Kept;
use super::packing::{CellLoad, Ordered, Packed, Packing, Run};

/// The shard of the release before that held each cell, by its domain and Morton code.
struct Owners<'a> {
    of: HashMap<(&'a str, u64), u32>,
}

impl<'a> Owners<'a> {
    fn new(previous: &'a Kept) -> Self {
        let mut of = HashMap::new();
        for (shard, held) in (0_u32..).zip(&previous.cells) {
            for (domain, codes) in held {
                for &code in codes {
                    of.insert((domain.as_str(), code), shard);
                }
            }
        }
        Self { of }
    }

    fn of(&self, domain: &str, code: u64) -> Option<u32> {
        self.of.get(&(domain, code)).copied()
    }
}

/// Packs the cells, given in `order`, and the buckets, keeping what `previous` placed. Each
/// shard's runs follow the cells in order, one run for each stretch of consecutive cells of one
/// domain that it holds. Shards left empty at the end are dropped.
pub(super) fn place(
    cells: &[CellLoad],
    order: &[Ordered],
    buckets: &[(u64, u64)],
    budget: u64,
    previous: &Kept,
) -> Packing {
    let mut fill = Fill {
        shards: vec![Packed::default(); previous.cells.len().max(1)],
        budget,
    };
    let owners = Owners::new(previous);
    let mut cell_shards: Vec<Option<u32>> = vec![None; cells.len()];
    for &((_, code), i) in order {
        let load = &cells[i];
        if let Some(owner) = owners.of(load.domain.1, code) {
            fill.take(owner, load.objects, load.bytes, 0);
            cell_shards[i] = Some(owner);
        }
    }
    let mut bucket_shards: Vec<Option<u32>> = vec![None; buckets.len()];
    for (k, &(objects, bytes)) in buckets.iter().enumerate() {
        let held = previous.loose.get(&(k as u64)).copied();
        if let Some(owner) = held.filter(|&o| (o as usize) < fill.shards.len()) {
            fill.take(owner, objects, bytes, objects);
            bucket_shards[k] = Some(owner);
        }
    }
    give_up(
        &mut fill,
        cells,
        order,
        buckets,
        &mut cell_shards,
        &mut bucket_shards,
    );
    for &(_, i) in order {
        if cell_shards[i].is_none() {
            let load = &cells[i];
            let index = fill.room(load.bytes);
            fill.take(index, load.objects, load.bytes, 0);
            cell_shards[i] = Some(index);
        }
    }
    for (k, &(objects, bytes)) in buckets.iter().enumerate() {
        if objects > 0 && bucket_shards[k].is_none() {
            let index = fill.room(bytes);
            fill.take(index, objects, bytes, objects);
            bucket_shards[k] = Some(index);
        }
    }
    let cell_shards: Vec<u32> = cell_shards.into_iter().map(|s| s.unwrap_or(0)).collect();
    fill.drop_empty_tail(&mut bucket_shards);
    let held = fill.runs(cells, order, &cell_shards);
    Packing {
        shards: fill.shards,
        cells: cell_shards,
        loose: bucket_shards,
        held,
    }
}

/// Makes each shard past the budget give up its buckets that hold objects, then its cells, last
/// first, until it is within it.
fn give_up(
    fill: &mut Fill,
    cells: &[CellLoad],
    order: &[Ordered],
    buckets: &[(u64, u64)],
    cell_shards: &mut [Option<u32>],
    bucket_shards: &mut [Option<u32>],
) {
    for (k, &(objects, bytes)) in buckets.iter().enumerate().rev() {
        if let Some(owner) = bucket_shards[k].filter(|&o| objects > 0 && fill.over(o)) {
            fill.take_back(owner, objects, bytes, objects);
            bucket_shards[k] = None;
        }
    }
    for &(_, i) in order.iter().rev() {
        if let Some(owner) = cell_shards[i].filter(|&o| fill.over(o)) {
            fill.take_back(owner, cells[i].objects, cells[i].bytes, 0);
            cell_shards[i] = None;
        }
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
        let shard = &mut self.shards[index as usize];
        shard.objects += objects;
        shard.unplaced += unplaced;
        shard.bytes += bytes;
    }

    /// Counts what [`Self::take`] counted into shard `index` out of it again.
    fn take_back(&mut self, index: u32, objects: u64, bytes: u64, unplaced: u64) {
        let shard = &mut self.shards[index as usize];
        shard.objects -= objects;
        shard.unplaced -= unplaced;
        shard.bytes -= bytes;
    }

    /// Whether shard `index` holds more than the budget.
    fn over(&self, index: u32) -> bool {
        self.shards[index as usize].bytes > self.budget
    }

    /// Drops the shards at the end that hold nothing, and the buckets' claim on them: always
    /// leaving one.
    fn drop_empty_tail(&mut self, bucket_shards: &mut [Option<u32>]) {
        let used = self
            .shards
            .iter()
            .rposition(|s| s.objects > 0)
            .map_or(1, |last| last + 1);
        self.shards.truncate(used);
        for shard in bucket_shards.iter_mut() {
            if shard.is_some_and(|s| s as usize >= self.shards.len()) {
                *shard = None;
            }
        }
    }

    /// Records each shard's runs of the cells, given in `order`, it holds by `cell_shards`, and
    /// gives the Morton codes of the cells each holds, by domain.
    fn runs(
        &mut self,
        cells: &[CellLoad],
        order: &[Ordered],
        cell_shards: &[u32],
    ) -> Vec<BTreeMap<String, Vec<u64>>> {
        let mut held = vec![BTreeMap::<String, Vec<u64>>::new(); self.shards.len()];
        let mut last: Option<u32> = None;
        for &((_, code), i) in order {
            let index = cell_shards[i];
            let domain = cells[i].domain.1;
            held[index as usize]
                .entry(domain.to_owned())
                .or_default()
                .push(code);
            let runs = &mut self.shards[index as usize].runs;
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
        held
    }
}
