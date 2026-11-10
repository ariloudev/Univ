//! The cell index that bounded volume queries read (query-a-bounded-volume).
//!
//! # Scheme
//!
//! Each cell space is a uniform grid of cubes in ICRS equatorial Cartesian coordinates about the
//! Solar System barycentre, anchored at the origin, in the space's unit. The cell of a point
//! `p` at level `L` is `floor(p_i / (edge · 2^L))` along each axis, so cells never depend on
//! what a release holds and the same cell names the same cube in every release.
//!
//! - **Objects.** A placed object is in the level-0 cell of its scale domain's space that holds
//!   its position: `stars` in parsecs with 10 pc cells, `nearby_galaxies` in megaparsecs with
//!   1 Mpc cells, `cosmic_web` in megaparsecs with 10 Mpc cells. A query of an axis-aligned box
//!   reads the cells whose index ranges [`CellSpace::range`] gives, one range per axis, and then
//!   tests each object's own position against the volume.
//! - **Structures.** A structure with a bounding box is in the `structures` space (Mpc, 10 Mpc
//!   cells at level 0) at the finest level at which its box meets at most
//!   [`STRUCTURE_SPAN`] cells along each axis, and in each cell of that level its box meets, so
//!   a structure of any size takes at most 64 rows. A query reads, for each level, the cells its
//!   box meets at that level ([`structure_ranges`]); a structure found is then tested against
//!   the volume by its geometry, which is how one whose centre lies outside the volume still
//!   matches.
//!
//! The index is a candidate filter: a cell meets the volume's box, not necessarily the volume.

use serde::{Deserialize, Serialize};

use crate::geometry::Aabb;
use crate::position::Vec3;

/// The most cells along one axis a structure's box meets at the level it is indexed at.
pub const STRUCTURE_SPAN: i64 = 4;
/// The coarsest level a structure is indexed at: cells of 10 Mpc · 2^20, beyond any structure.
pub const MAX_STRUCTURE_LEVEL: u8 = 20;

/// A space of cells, with its unit and level-0 edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellSpace {
    Stars,
    NearbyGalaxies,
    CosmicWeb,
    Structures,
    GalaxyTracers,
}

/// Every cell space, in the order a release's manifest lists them: Galaxy tracers, added for
/// release 13, last, so that every earlier space keeps its place in the packing order.
pub const SPACES: [CellSpace; 5] = [
    CellSpace::Stars,
    CellSpace::NearbyGalaxies,
    CellSpace::CosmicWeb,
    CellSpace::Structures,
    CellSpace::GalaxyTracers,
];

/// A cell's indices along x, y and z.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Cell {
    pub ix: i64,
    pub iy: i64,
    pub iz: i64,
}

/// The cells from `min` to `max` along each axis, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRange {
    pub min: Cell,
    pub max: Cell,
}

impl CellRange {
    /// The number of cells along each axis.
    #[must_use]
    pub fn spans(&self) -> [i64; 3] {
        [
            self.max.ix - self.min.ix + 1,
            self.max.iy - self.min.iy + 1,
            self.max.iz - self.min.iz + 1,
        ]
    }

    /// Whether `cell` lies in the range.
    #[must_use]
    pub fn contains(&self, cell: Cell) -> bool {
        (self.min.ix..=self.max.ix).contains(&cell.ix)
            && (self.min.iy..=self.max.iy).contains(&cell.iy)
            && (self.min.iz..=self.max.iz).contains(&cell.iz)
    }

    /// Every cell of the range, x slowest.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (self.min.ix..=self.max.ix).flat_map(move |ix| {
            (self.min.iy..=self.max.iy)
                .flat_map(move |iy| (self.min.iz..=self.max.iz).map(move |iz| Cell { ix, iy, iz }))
        })
    }
}

impl CellSpace {
    /// Its name in the index, as the tile manifests name the scale domains.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Stars => "stars",
            Self::NearbyGalaxies => "nearby_galaxies",
            Self::CosmicWeb => "cosmic_web",
            Self::Structures => "structures",
            Self::GalaxyTracers => "galaxy_tracers",
        }
    }

    /// The symbol of its unit: `pc` for stars and Galaxy tracers, `Mpc` otherwise.
    #[must_use]
    pub fn unit(self) -> &'static str {
        match self {
            Self::Stars | Self::GalaxyTracers => "pc",
            _ => "Mpc",
        }
    }

    /// The edge of a level-0 cell, in its unit.
    #[must_use]
    pub fn edge(self) -> f64 {
        match self {
            Self::Stars | Self::CosmicWeb | Self::Structures => 10.0,
            Self::NearbyGalaxies => 1.0,
            // Some 150,000 tracers spread over the whole Galaxy: cells ten times a star's.
            Self::GalaxyTracers => 100.0,
        }
    }

    /// The edge of a cell at `level`, in its unit.
    #[must_use]
    pub fn edge_at(self, level: u8) -> f64 {
        self.edge() * f64::from(1_u32 << level.min(31))
    }

    /// The cell at `level` holding `p`, in its unit; none for a coordinate that is not finite.
    #[must_use]
    pub fn cell_at(self, level: u8, p: Vec3) -> Option<Cell> {
        let edge = self.edge_at(level);
        let index = |c: f64| {
            let i = (c / edge).floor();
            // Within ±2^53, so exact; beyond any position a release holds.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked to lie within ±2^53 first"
            )]
            (i.is_finite() && i.abs() < 9.0e15).then_some(i as i64)
        };
        Some(Cell {
            ix: index(p[0])?,
            iy: index(p[1])?,
            iz: index(p[2])?,
        })
    }

    /// The level-0 cell holding `p`.
    #[must_use]
    pub fn cell(self, p: Vec3) -> Option<Cell> {
        self.cell_at(0, p)
    }

    /// The cells at `level` that the box `bounds` meets, in its unit.
    #[must_use]
    pub fn range_at(self, level: u8, bounds: &Aabb) -> Option<CellRange> {
        Some(CellRange {
            min: self.cell_at(level, bounds.min)?,
            max: self.cell_at(level, bounds.max)?,
        })
    }

    /// The level-0 cells that the box `bounds` meets: the cells an object query reads.
    #[must_use]
    pub fn range(self, bounds: &Aabb) -> Option<CellRange> {
        self.range_at(0, bounds)
    }
}

/// The level a structure with box `bounds` (ICRS, Mpc) is indexed at, and the cells it is in:
/// the finest level at which the box meets at most [`STRUCTURE_SPAN`] cells along each axis.
/// None for a box that is not finite or is beyond [`MAX_STRUCTURE_LEVEL`].
#[must_use]
pub fn structure_cells(bounds: &Aabb) -> Option<(u8, CellRange)> {
    (0..=MAX_STRUCTURE_LEVEL).find_map(|level| {
        let range = CellSpace::Structures.range_at(level, bounds)?;
        range
            .spans()
            .iter()
            .all(|&n| n <= STRUCTURE_SPAN)
            .then_some((level, range))
    })
}

/// The cells of each structure level that a query box `bounds` (ICRS, Mpc) meets.
#[must_use]
pub fn structure_ranges(bounds: &Aabb) -> Vec<(u8, CellRange)> {
    (0..=MAX_STRUCTURE_LEVEL)
        .filter_map(|level| Some((level, CellSpace::Structures.range_at(level, bounds)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(min: Vec3, max: Vec3) -> Aabb {
        Aabb { min, max }
    }

    #[test]
    fn a_point_falls_in_the_cell_below_it_on_every_axis() {
        let s = CellSpace::CosmicWeb;
        assert_eq!(
            s.cell([0.0, 9.999, -0.001]),
            Some(Cell {
                ix: 0,
                iy: 0,
                iz: -1
            })
        );
        assert_eq!(
            s.cell([10.0, -10.0, 25.0]),
            Some(Cell {
                ix: 1,
                iy: -1,
                iz: 2
            })
        );
        assert_eq!(
            CellSpace::NearbyGalaxies.cell([2.5, 0.0, 0.0]).unwrap().ix,
            2
        );
        assert_eq!(s.cell([f64::NAN, 0.0, 0.0]), None);
    }

    #[test]
    fn a_query_box_reads_every_cell_holding_a_point_inside_it() {
        let s = CellSpace::Stars;
        let query = aabb([-35.0, 3.0, 0.0], [12.0, 41.0, 9.0]);
        let range = s.range(&query).unwrap();
        for p in [[-35.0, 3.0, 0.0], [12.0, 41.0, 9.0], [0.0, 20.0, 5.0]] {
            assert!(range.contains(s.cell(p).unwrap()), "{p:?}");
        }
        assert_eq!(range.spans(), [6, 5, 1]);
        assert_eq!(range.cells().count(), 30);
    }

    #[test]
    fn a_structure_takes_at_most_four_cells_a_side_at_its_level() {
        // 25 Mpc fits level 0 (cells 0 to 2). 1,500 Mpc along x meets 6 cells of 320 Mpc
        // (level 5, cells -3 to 2) and 4 of 640 Mpc (level 6, cells -2 to 1).
        let small = aabb([0.0, 0.0, 0.0], [25.0, 5.0, 5.0]);
        assert_eq!(structure_cells(&small).unwrap().0, 0);
        let large = aabb([-750.0, 100.0, 0.0], [750.0, 200.0, 10.0]);
        let (level, range) = structure_cells(&large).unwrap();
        assert_eq!(level, 6);
        assert_eq!(range.spans(), [4, 1, 1]);
        let finer = CellSpace::Structures.range_at(level - 1, &large).unwrap();
        assert!(finer.spans().iter().any(|&n| n > STRUCTURE_SPAN));
        // A query box inside the structure finds it at its level.
        let query = aabb([0.0, 150.0, 0.0], [1.0, 151.0, 1.0]);
        let ranges = structure_ranges(&query);
        let (_, at) = ranges.iter().find(|(l, _)| *l == level).unwrap();
        assert!(range.cells().any(|c| at.contains(c)));
    }
}
