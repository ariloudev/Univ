//! A release's objects written to their object shards (build-a-release): each object's rows,
//! with its cell, to the shard of its cell, or of its id when it is not placed, and its shard
//! to the catalogue's `object_shards`.
//!
//! The shards are planned before anything is written: a first pass turns every object into its
//! rows without writing them, only adding up the D1 bytes they would take in a shard
//! ([`D1Writer::measuring`]), by cell; the cells are then packed ([`super::packing`]). Both
//! passes write each object the same way ([`write_object`]), so the bytes packed are the bytes
//! written. The objects' ids are checked unique once, beforehand ([`repeated_id`]), so the
//! writer forgets each object's keys once its rows are written.

use univ_model::cosmology::Cosmology;
use univ_model::release::manifest::{D1Estimate, ShardCells, ShardManifest};
use univ_model::release::rows::{ObjectCellRow, ObjectShardRow, ReleaseShardRow, ShardCellRow};
use univ_model::release::shards::Database;

use super::cells::{DomainCell, ObjectCells};
use super::d1::D1Writer;
use super::entries::Numbering;
use super::kept::Kept;
use super::objects::ObjectWriter;
use super::packing::{CellLoad, Packing, pack};
use super::terms::Terms;
use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::watch::Watch;

/// Objects turned into rows between two looks at the build's stop flag.
pub const OBJECTS_BETWEEN_CHECKS: usize = 10_000;

/// The first id two of `objects` share, if any.
#[must_use]
pub fn repeated_id(objects: &[ReleaseObject]) -> Option<u64> {
    let mut ids: Vec<u64> = objects.iter().map(|o| o.id.get()).collect();
    ids.sort_unstable();
    ids.windows(2).find(|w| w[0] == w[1]).map(|w| w[0])
}

/// What the objects are written with: their definitions and cosmology, and their cells.
pub struct Sharding<'a> {
    pub definitions: &'a [SourceDefinition],
    pub cosmology: &'a Cosmology,
    pub cells: &'a ObjectCells,
}

impl Sharding<'_> {
    /// The cell of the object at `place`, with its place among the cells.
    fn cell(&self, place: usize) -> Option<(usize, &DomainCell)> {
        self.cells.of(place).map(|i| (i, &self.cells.cells[i]))
    }

    /// Plans the shards of `objects` within `budget` estimated bytes each, `numbering` being the
    /// release's numbering as the objects' rows will begin from it.
    ///
    /// # Errors
    ///
    /// Fails when a row cannot be made, a cell alone passes the budget, or the owner asked the
    /// build to stop.
    pub fn plan(
        &self,
        objects: &[ReleaseObject],
        numbering: &Numbering,
        budget: u64,
        previous: &Kept,
        watch: Watch<'_>,
    ) -> Result<Packing, String> {
        let mut writer = D1Writer::measuring(1);
        let mut rows = ObjectWriter::new(numbering.clone(), self.definitions, self.cosmology);
        let mut loads: Vec<CellLoad> = self
            .cells
            .cells
            .iter()
            .map(|c| CellLoad {
                domain: (c.place, c.domain),
                cell: c.cell,
                ..CellLoad::default()
            })
            .collect();
        let mut loose = Vec::new();
        for (place, o) in objects.iter().enumerate() {
            if place % OBJECTS_BETWEEN_CHECKS == 0 {
                watch.check()?;
            }
            let before = writer.estimated_bytes_in(Database::Shard(0));
            let at = self.cell(place);
            write_object(&mut rows, &mut writer, o, at.map(|(_, c)| c), 0)?;
            let bytes = writer.estimated_bytes_in(Database::Shard(0)) - before;
            match at {
                Some((i, _)) => {
                    loads[i].objects += 1;
                    loads[i].bytes += bytes;
                }
                None => loose.push((o.id.get(), bytes)),
            }
        }
        pack(&loads, &loose, budget, previous).map_err(|e| e.to_string())
    }

    /// Writes every object of `objects` through `writer` to the shard `packing` gives it, with
    /// `rows` numbering them.
    ///
    /// # Errors
    ///
    /// Fails when a row cannot be made or written, or the owner asked the build to stop.
    pub fn write(
        &self,
        objects: &[ReleaseObject],
        rows: &mut ObjectWriter<'_>,
        writer: &mut D1Writer<'_>,
        packing: &Packing,
        watch: Watch<'_>,
    ) -> Result<(), String> {
        for (place, o) in objects.iter().enumerate() {
            if place % OBJECTS_BETWEEN_CHECKS == 0 {
                watch.check()?;
            }
            let at = self.cell(place);
            let shard = match at {
                Some((i, _)) => packing.shard_of_cell(i),
                None => packing.shard_of_unplaced(o.id.get()),
            };
            write_object(rows, writer, o, at.map(|(_, c)| c), shard)?;
        }
        writer.clear_shard();
        Ok(())
    }
}

/// Writes the object `o` with its rows to `shard`: its own rows, its cell `at` when it is
/// placed, and its shard in the catalogue.
fn write_object(
    rows: &mut ObjectWriter<'_>,
    writer: &mut D1Writer<'_>,
    o: &ReleaseObject,
    at: Option<&DomainCell>,
    shard: u32,
) -> Result<(), String> {
    writer.set_shard(shard);
    rows.write(writer, o)?;
    let release = rows.numbering.release;
    if let Some(c) = at {
        writer.add(&ObjectCellRow {
            release,
            domain: rows.numbering.terms.id(c.domain),
            ix: c.cell.ix,
            iy: c.cell.iy,
            iz: c.cell.iz,
            object_id: o.id.get(),
        })?;
    }
    writer.add(&ObjectShardRow {
        release,
        id: o.id.get(),
        shard,
    })?;
    writer.forget_object_keys();
    Ok(())
}

/// Writes the catalogue's index of the object shards of release `number`: each shard's runs
/// of cells, by their domain's term, and its objects and estimate.
pub fn write_shard_index(
    writer: &mut D1Writer<'_>,
    number: u32,
    packing: &Packing,
    terms: &mut Terms,
) -> Result<(), String> {
    for (index, shard) in (0_u32..).zip(&packing.shards) {
        for run in &shard.runs {
            writer.add(&ShardCellRow {
                release: number,
                shard: index,
                domain: terms.id(run.domain),
                first: run.first,
                last: run.last,
            })?;
        }
        writer.add(&ReleaseShardRow {
            release: number,
            shard: index,
            objects: shard.objects,
            estimated_bytes: writer.estimated_bytes_in(Database::Shard(index)),
        })?;
    }
    Ok(())
}

/// Each object shard as the manifest names it, with its estimate.
pub fn shard_manifests(packing: &Packing, estimates: Vec<D1Estimate>) -> Vec<ShardManifest> {
    (0_u32..)
        .zip(&packing.shards)
        .zip(estimates)
        .map(|((index, shard), d1)| ShardManifest {
            index,
            objects: shard.objects,
            unplaced: shard.unplaced,
            cells: shard
                .runs
                .iter()
                .map(|r| ShardCells {
                    domain: r.domain.to_owned(),
                    first: r.first,
                    last: r.last,
                    cells: r.cells,
                })
                .collect(),
            d1,
        })
        .collect()
}
