//! The cell index of a release (AC-85), laid out as `univ_model::release::cells` describes:
//! each placed object in the level-0 cell of its scale domain holding its position, the same
//! position its tile gives it, and each structure with a bounding box in the cells of the
//! structures space that its box meets.
//!
//! A structure enters the index with the box `super::place` gives it under the release
//! cosmology: a member list placed whole, a point, a sphere, or a sky region bounded between its
//! near and far distances. One that cannot be placed is not in the index; the build lists it
//! with the reason.

use std::collections::HashMap;

use univ_model::cosmology::Cosmology;
use univ_model::geometry::Aabb;
use univ_model::release::cells::structure_cells;
use univ_model::release::rows::StructureCellRow;
use univ_model::release::{Cell, cells::SPACES};

use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::tiles::{assign, name_of};

/// A level-0 cell of a scale domain that holds a placed object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainCell {
    /// The domain's place among the cell spaces, `univ_model::release::cells::SPACES`.
    pub place: usize,
    pub domain: &'static str,
    pub cell: Cell,
}

/// The level-0 cell of each placed object of a release, by the object's place among the
/// release's objects, and the distinct cells that hold one.
#[derive(Debug, Default)]
pub struct ObjectCells {
    /// Each object's cell, its place in `cells`; [`NOT_PLACED`] for an object not placed.
    of: Vec<u32>,
    /// The distinct cells, in the order first met.
    pub cells: Vec<DomainCell>,
}

/// An object not placed, among [`ObjectCells`]'s.
const NOT_PLACED: u32 = u32::MAX;

impl ObjectCells {
    /// The place among [`ObjectCells::cells`] of the cell of the object at `place` among the
    /// release's objects: none for an object not placed.
    #[must_use]
    pub fn of(&self, place: usize) -> Option<usize> {
        self.of
            .get(place)
            .filter(|&&c| c != NOT_PLACED)
            .map(|&c| c as usize)
    }
}

/// The rows of a release's cell index of structures; the objects' cells are written with the
/// objects, to their shards.
#[derive(Debug, Default)]
pub struct CellRows {
    pub structures: Vec<StructureCellRow>,
    /// The structures with a bounding box, which the index holds.
    pub structures_indexed: u64,
    /// Each structure the index does not hold, by its id, with the reason.
    pub not_indexed: Vec<(String, String)>,
}

/// The level-0 cell of each placed object of `objects`, in the scale domains `definitions` give
/// them under `cosmology`.
///
/// # Errors
///
/// Fails when an object cannot be given its scale domain, a position is not finite, or the
/// objects are more than a `u32` counts.
pub fn object_cells(
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
    cosmology: &Cosmology,
) -> Result<ObjectCells, String> {
    let assigned = assign(objects, definitions, cosmology).map_err(|e| e.to_string())?;
    let mut cells = ObjectCells {
        of: vec![NOT_PLACED; objects.len()],
        cells: Vec::new(),
    };
    let mut seen: HashMap<(usize, Cell), u32> = HashMap::new();
    for (domain, points) in &assigned.domains {
        let name = name_of(*domain);
        let place = SPACES
            .iter()
            .position(|s| s.name() == name)
            .ok_or_else(|| format!("no cell space for {name}"))?;
        for p in &points.points {
            let cell = SPACES[place]
                .cell(p.position)
                .ok_or_else(|| format!("object {}'s position has no cell", p.id))?;
            let next = u32::try_from(cells.cells.len())
                .ok()
                .filter(|&n| n != NOT_PLACED)
                .ok_or("more cells than a u32 counts")?;
            let at = *seen.entry((place, cell)).or_insert_with(|| {
                cells.cells.push(DomainCell {
                    place,
                    domain: name,
                    cell,
                });
                next
            });
            cells.of[p.place as usize] = at;
        }
    }
    Ok(cells)
}

/// The cell index of release `release` of the `structures`, each by its id with its ICRS
/// bounding box in Mpc or the reason it has none.
///
/// # Errors
///
/// Fails when a structure's bounding box has no cells.
pub fn index_rows(
    release: u32,
    structures: &[(String, Result<Aabb, String>)],
) -> Result<CellRows, String> {
    let mut rows = CellRows::default();
    for (id, bounds) in structures {
        let bounds = match bounds {
            Ok(bounds) => bounds,
            Err(reason) => {
                rows.not_indexed.push((id.clone(), reason.clone()));
                continue;
            }
        };
        let (level, range) = structure_cells(bounds)
            .ok_or_else(|| format!("structure {id}'s bounding box has no cells"))?;
        rows.structures_indexed += 1;
        for cell in range.cells() {
            rows.structures.push(StructureCellRow {
                release,
                level,
                ix: cell.ix,
                iy: cell.iy,
                iz: cell.iz,
                structure: id.clone(),
            });
        }
    }
    Ok(rows)
}
