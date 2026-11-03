//! The cell index of a release (AC-85), laid out as `univ_model::release::cells` describes:
//! each placed object in the level-0 cell of its scale domain holding its position, the same
//! position its tile gives it, and each structure with a bounding box in the cells of the
//! structures space that its box meets.
//!
//! A structure enters the index with the box `super::place` gives it under the release
//! cosmology: a member list placed whole, a point, a sphere, or a sky region bounded between its
//! near and far distances. One that cannot be placed is not in the index; the build lists it
//! with the reason.

use univ_model::cosmology::Cosmology;
use univ_model::geometry::Aabb;
use univ_model::release::cells::structure_cells;
use univ_model::release::rows::{ObjectCellRow, StructureCellRow};
use univ_model::release::{CellSpace, cells::SPACES};

use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::tiles::{assign, name_of};

/// The rows of a release's cell index.
#[derive(Debug, Default)]
pub struct CellRows {
    pub objects: Vec<ObjectCellRow>,
    pub structures: Vec<StructureCellRow>,
    /// The structures with a bounding box, which the index holds.
    pub structures_indexed: u64,
    /// Each structure the index does not hold, by its id, with the reason.
    pub not_indexed: Vec<(String, String)>,
}

/// The cell space of the scale domain named `name`.
fn space(name: &str) -> Option<CellSpace> {
    SPACES.into_iter().find(|s| s.name() == name)
}

/// The cell index of release `release`: the placed `objects`, in the scale domains
/// `definitions` give them under `cosmology`, and the `structures`, each by its id with its
/// ICRS bounding box in Mpc or the reason it has none.
///
/// # Errors
///
/// Fails when an object cannot be given its scale domain or a position is not finite.
pub fn cell_rows(
    release: u32,
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
    cosmology: &Cosmology,
    structures: &[(String, Result<Aabb, String>)],
) -> Result<CellRows, String> {
    let assigned = assign(objects, definitions, cosmology).map_err(|e| e.to_string())?;
    let mut rows = CellRows::default();
    for (domain, points) in &assigned.domains {
        let name = name_of(*domain);
        let space = space(name).ok_or_else(|| format!("no cell space for {name}"))?;
        for p in &points.points {
            let cell = space
                .cell(p.position)
                .ok_or_else(|| format!("object {}'s position has no cell", p.id))?;
            rows.objects.push(ObjectCellRow {
                release,
                domain: name.to_owned(),
                ix: cell.ix,
                iy: cell.iy,
                iz: cell.iz,
                object_id: p.id.get(),
            });
        }
    }
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
