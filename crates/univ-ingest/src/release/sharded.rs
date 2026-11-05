//! A release's objects written to their object shards (build-a-release): each object's rows,
//! with its cell, to the shard of its cell, or of its id when it is not placed, and its shard
//! to the catalogue's `object_shards`.
//!
//! The shards are planned before anything is written: a first pass turns every object into its
//! rows without writing them, only adding up the D1 bytes they would take in a shard
//! ([`D1Writer::measuring`]), by cell; the cells are then packed ([`super::packing`]). Both
//! passes write each object the same way ([`write_object`]), so the bytes packed are the bytes
//! written.

use std::collections::HashMap;

use univ_model::cosmology::Cosmology;
use univ_model::release::cells::SPACES;
use univ_model::release::manifest::{D1Estimate, ShardCells, ShardManifest};
use univ_model::release::rows::{ObjectCellRow, ObjectShardRow, ReleaseShardRow, ShardCellRow};
use univ_model::release::shards::Database;
use univ_model::release::{Cell, Row};

use super::cells::ObjectCell;
use super::d1::D1Writer;
use super::entries::Numbering;
use super::objects::ObjectWriter;
use super::packing::{CellLoad, Packing, pack};
use super::terms::Terms;
use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::watch::Watch;

/// Objects turned into rows between two looks at the build's stop flag.
const OBJECTS_BETWEEN_CHECKS: usize = 10_000;

/// The cell of each placed object, sorted by its id, with its domain's place in the packing
/// order.
pub struct Cells(Vec<(u64, usize, ObjectCell)>);

impl Cells {
    /// The cells of `cells`, by object id.
    #[must_use]
    pub fn new(cells: Vec<ObjectCell>) -> Self {
        let mut by_id: Vec<(u64, usize, ObjectCell)> = cells
            .into_iter()
            .map(|c| {
                let place = SPACES
                    .iter()
                    .position(|s| s.name() == c.domain)
                    .unwrap_or(SPACES.len());
                (c.object_id, place, c)
            })
            .collect();
        by_id.sort_unstable_by_key(|(id, _, _)| *id);
        Self(by_id)
    }

    /// The cell of the object `id`, with its domain's place: none for an object not placed.
    fn of(&self, id: u64) -> Option<(usize, &ObjectCell)> {
        let i = self.0.binary_search_by_key(&id, |(k, _, _)| *k).ok()?;
        let (_, place, cell) = &self.0[i];
        Some((*place, cell))
    }

    /// The cells, as the cell index holds them.
    #[must_use]
    pub fn into_cells(self) -> Vec<ObjectCell> {
        self.0.into_iter().map(|(_, _, c)| c).collect()
    }
}

fn cell(c: &ObjectCell) -> Cell {
    Cell {
        ix: c.ix,
        iy: c.iy,
        iz: c.iz,
    }
}

/// What the objects are written with: their definitions and cosmology, and the cells.
pub struct Sharding<'a> {
    pub definitions: &'a [SourceDefinition],
    pub cosmology: &'a Cosmology,
    pub cells: &'a Cells,
}

impl Sharding<'_> {
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
        watch: Watch<'_>,
    ) -> Result<Packing, String> {
        let mut writer = D1Writer::measuring(1);
        let mut rows = ObjectWriter::new(numbering.clone(), self.definitions, self.cosmology);
        let mut loads: HashMap<(usize, Cell), CellLoad> = HashMap::new();
        let mut loose = Vec::new();
        for (n, o) in objects.iter().enumerate() {
            if n % OBJECTS_BETWEEN_CHECKS == 0 {
                watch.check()?;
            }
            let before = writer.estimated_bytes_in(Database::Shard(0));
            let at = self.cells.of(o.id.get());
            write_object(&mut rows, &mut writer, o, at, 0)?;
            let bytes = writer.estimated_bytes_in(Database::Shard(0)) - before;
            match at {
                Some((place, c)) => {
                    let load = loads.entry((place, cell(c))).or_insert(CellLoad {
                        domain: (place, c.domain),
                        cell: cell(c),
                        objects: 0,
                        bytes: 0,
                    });
                    load.objects += 1;
                    load.bytes += bytes;
                }
                None => loose.push((o.id.get(), bytes)),
            }
        }
        let loads: Vec<CellLoad> = loads.into_values().collect();
        pack(&loads, &loose, budget).map_err(|e| e.to_string())
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
        for (n, o) in objects.iter().enumerate() {
            if n % OBJECTS_BETWEEN_CHECKS == 0 {
                watch.check()?;
            }
            let at = self.cells.of(o.id.get());
            let shard = match at {
                Some((place, c)) => packing
                    .shard_of_cell(place, cell(c))
                    .ok_or_else(|| format!("object {}'s cell was not packed", o.id))?,
                None => packing.shard_of_unplaced(o.id.get()),
            };
            write_object(rows, writer, o, at, shard)?;
        }
        Ok(())
    }
}

/// Writes the object `o` with its rows to `shard`: its own rows, its cell `at` when it is
/// placed, and its shard in the catalogue.
fn write_object(
    rows: &mut ObjectWriter<'_>,
    writer: &mut D1Writer<'_>,
    o: &ReleaseObject,
    at: Option<(usize, &ObjectCell)>,
    shard: u32,
) -> Result<(), String> {
    writer.set_shard(shard);
    rows.write(writer, o)?;
    let release = rows.numbering.release;
    if let Some((_, c)) = at {
        writer.add(&ObjectCellRow {
            release,
            domain: rows.numbering.terms.id(c.domain),
            ix: c.ix,
            iy: c.iy,
            iz: c.iz,
            object_id: c.object_id,
        })?;
    }
    writer.add(&ObjectShardRow {
        release,
        id: o.id.get(),
        shard,
    })?;
    writer.forget_keys(&[ObjectShardRow::TABLE.name, ObjectCellRow::TABLE.name]);
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
    for (i, shard) in packing.shards.iter().enumerate() {
        let index = u32::try_from(i).unwrap_or(u32::MAX);
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
    packing
        .shards
        .iter()
        .zip(estimates)
        .enumerate()
        .map(|(i, (shard, d1))| ShardManifest {
            index: u32::try_from(i).unwrap_or(u32::MAX),
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
