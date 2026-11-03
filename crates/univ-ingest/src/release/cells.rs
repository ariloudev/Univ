//! The cell index of a release (AC-85), laid out as `univ_model::release::cells` describes:
//! each placed object in the level-0 cell of its scale domain holding its position, the same
//! position its tile gives it, and each structure with a bounding box in the cells of the
//! structures space that its box meets.
//!
//! A structure has a bounding box once its geometry is placed in three dimensions: in this
//! release, a member list whose members are all placed, bounded by their positions. A structure
//! shown at its published position, a primitive solid or a sky region is not yet placed under
//! the release cosmology, so it is not in the index; the build counts it. A sky region's box is
//! the sphere of its far distance, far too coarse for an index: when sky regions are placed, they
//! need a bound of their cone between the near and far distances instead.

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
}

/// The cell space of the scale domain named `name`.
fn space(name: &str) -> Option<CellSpace> {
    SPACES.into_iter().find(|s| s.name() == name)
}

/// The cell index of release `release`: the placed `objects`, in the scale domains
/// `definitions` give them under `cosmology`, and the `structures`, each by its id with its
/// ICRS bounding box in Mpc, if it has one.
///
/// # Errors
///
/// Fails when an object cannot be given its scale domain or a position is not finite.
pub fn cell_rows(
    release: u32,
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
    cosmology: &Cosmology,
    structures: &[(String, Option<Aabb>)],
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
        let Some(bounds) = bounds else { continue };
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
