//! Volume grids: one or more fields sampled on a regular Cartesian grid in a named frame, with the
//! order of the array axes, the cell spacing and the origin made explicit.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cosmology::AssumedCosmology;
use crate::position::{Epoch, Frame, Vec3};
use crate::units::{Dimension, Unit};
use crate::value::{KindOfValue, Quantity, Uncertainty, ValueError, positive};

/// A Cartesian axis of a frame: X, Y or Z of that frame, such as SGX in supergalactic coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// The three axes in frame order.
    pub const XYZ: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// The axis's place in a frame vector: 0 for X, 1 for Y, 2 for Z.
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Where a grid lies: its frame, the axis each array dimension runs along, its shape, spacing and
/// origin. Positions and the spacing are in `unit`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGridGeometry")]
pub struct GridGeometry {
    pub frame: Frame,
    pub epoch: Epoch,
    /// A unit of length, or h⁻¹ Mpc.
    pub unit: Unit,
    /// The frame axis that each array dimension runs along, slowest first: `[X, Y, Z]` means
    /// `samples[i_X][i_Y][i_Z]` in row-major (C) order, Z running fastest.
    pub axes: [Axis; 3],
    /// Cells along each array dimension, in the order of `axes`.
    pub shape: [usize; 3],
    /// The edge of one cubic cell.
    pub cell: f64,
    /// The position, in frame order (X, Y, Z), of the centre of the cell whose indices are all 0.
    pub origin: Vec3,
    /// The cosmology the reconstruction assumed, where its source states one: its Hubble
    /// constant turns h⁻¹ Mpc into the source's Mpc. `None` when no value is stated.
    pub assumed_cosmology: Option<AssumedCosmology>,
}

/// A grid geometry as read, before its checks.
#[derive(Deserialize)]
struct RawGridGeometry {
    frame: Frame,
    epoch: Epoch,
    unit: Unit,
    axes: [Axis; 3],
    shape: [usize; 3],
    cell: f64,
    origin: Vec3,
    assumed_cosmology: Option<AssumedCosmology>,
}

impl TryFrom<RawGridGeometry> for GridGeometry {
    type Error = GridError;
    fn try_from(raw: RawGridGeometry) -> Result<Self, GridError> {
        let geometry = Self {
            frame: raw.frame,
            epoch: raw.epoch,
            unit: raw.unit,
            axes: raw.axes,
            shape: raw.shape,
            cell: raw.cell,
            origin: raw.origin,
            assumed_cosmology: raw.assumed_cosmology,
        };
        geometry.check()?;
        Ok(geometry)
    }
}

/// A grid or one of its fields that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum GridError {
    #[error("{0:?} is not a unit of length")]
    NotALength(Unit),
    #[error("the axes {0:?} do not name X, Y and Z once each")]
    Axes([Axis; 3]),
    #[error("the grid needs a positive finite cell size, a finite origin and cells on every axis")]
    Malformed,
    #[error("the field {field} holds {found} samples where its grid needs {expected}")]
    Samples {
        field: String,
        expected: usize,
        found: usize,
    },
    #[error(transparent)]
    Value(#[from] ValueError),
}

impl GridGeometry {
    /// Checks the geometry: a length unit, each axis once, cells on every axis and no more than
    /// a vector field's samples can be counted, and a positive finite spacing and Hubble
    /// constant.
    ///
    /// # Errors
    ///
    /// Fails on the first check that does not hold.
    pub fn check(&self) -> Result<(), GridError> {
        if !matches!(
            self.unit.dimension(),
            Dimension::Length | Dimension::HubbleScaledLength
        ) {
            return Err(GridError::NotALength(self.unit));
        }
        if Axis::XYZ.iter().any(|a| !self.axes.contains(a)) {
            return Err(GridError::Axes(self.axes));
        }
        let hubble = self.assumed_cosmology.is_none_or(|a| a.check().is_ok());
        // A vector field holds three samples a cell, and that count must fit in memory.
        let samples = self
            .shape
            .iter()
            .try_fold(3_usize, |n, &cells| n.checked_mul(cells));
        if !positive(self.cell)
            || !self.origin.iter().all(|c| c.is_finite())
            || self.shape.contains(&0)
            || samples.is_none()
            || !hubble
        {
            return Err(GridError::Malformed);
        }
        Ok(())
    }

    /// The number of cells.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.shape.iter().product()
    }

    /// The number of cells along a frame axis.
    #[must_use]
    pub fn cells_along(&self, axis: Axis) -> usize {
        self.axes
            .iter()
            .zip(self.shape)
            .find_map(|(a, n)| (*a == axis).then_some(n))
            .unwrap_or(0)
    }

    /// Where the sample of a cell sits in the samples, the cell given by its indices along X, Y
    /// and Z of the frame; `None` outside the grid.
    #[must_use]
    pub fn sample_index(&self, cell: [usize; 3]) -> Option<usize> {
        self.axes
            .iter()
            .zip(self.shape)
            .try_fold(0, |flat, (axis, n)| {
                let i = cell[axis.index()];
                (i < n).then_some(flat * n + i)
            })
    }

    /// The centre of a cell given by its indices along X, Y and Z, in frame order.
    #[must_use]
    pub fn centre(&self, cell: [usize; 3]) -> Vec3 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "cell indices are far below 2^52"
        )]
        let at = |k: usize| self.origin[k] + cell[k] as f64 * self.cell;
        [at(0), at(1), at(2)]
    }

    /// The cell whose centre is nearest a position given in frame order; `None` outside the grid.
    #[must_use]
    pub fn cell_at(&self, position: Vec3) -> Option<[usize; 3]> {
        let mut cell = [0; 3];
        for axis in Axis::XYZ {
            let k = axis.index();
            let i = ((position[k] - self.origin[k]) / self.cell + 0.5).floor();
            #[expect(clippy::cast_precision_loss, reason = "cell counts are far below 2^52")]
            let inside = i >= 0.0 && i < self.cells_along(axis) as f64;
            if !inside {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "i is a whole number in [0, cells)"
            )]
            let i = i as usize;
            cell[k] = i;
        }
        Some(cell)
    }

    /// The corner of the grid, where its first cell begins: the origin of a
    /// [`crate::geometry::Geometry::VolumeGrid`].
    #[must_use]
    pub fn corner(&self) -> Vec3 {
        self.origin.map(|c| c - self.cell / 2.0)
    }
}

/// How many values a field holds per cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Components {
    /// One value per cell.
    Scalar,
    /// A vector per cell, its components along these frame axes. The component is the slowest
    /// index of the samples: all cells of the first component, then the second, then the third.
    Vector { axes: [Axis; 3] },
}

impl Components {
    /// Values per cell.
    #[must_use]
    pub fn count(self) -> usize {
        match self {
            Self::Scalar => 1,
            Self::Vector { .. } => 3,
        }
    }
}

/// The samples of a field: real numbers, or the integer labels of a segmentation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Samples {
    Real(Vec<f64>),
    Label(Vec<u32>),
}

impl Samples {
    fn len(&self) -> usize {
        match self {
            Self::Real(v) => v.len(),
            Self::Label(v) => v.len(),
        }
    }
}

/// The uncertainty a field's source published for each cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum CellUncertainty {
    /// The source published none: not the same as an uncertainty of zero.
    NoneReported,
    /// A one-sigma standard deviation per sample, laid out as the samples, from the source's
    /// column `column`.
    StandardDeviation { column: String, sigma: Vec<f64> },
}

/// The name a segmentation gives one of its labels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelName {
    pub label: u32,
    pub name: String,
}

/// One quantity sampled on a grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// The source's column, array or member this field was read from.
    pub column: String,
    /// The field it fills in the source definition, such as `density` or `basin_label`.
    pub field: String,
    pub unit: Unit,
    pub kind: KindOfValue,
    pub components: Components,
    pub samples: Samples,
    pub uncertainty: CellUncertainty,
    /// The name of each label of a segmentation that its source names; empty otherwise.
    pub labels: Vec<LabelName>,
}

/// A volume grid of one source: its geometry and the fields sampled on it. One read back is
/// checked as [`Grid::new`] checks one built.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGrid")]
pub struct Grid {
    /// The source definition's id.
    pub source: String,
    pub geometry: GridGeometry,
    pub fields: Vec<Field>,
}

/// A grid as read, before its checks.
#[derive(Deserialize)]
struct RawGrid {
    source: String,
    geometry: GridGeometry,
    fields: Vec<Field>,
}

impl TryFrom<RawGrid> for Grid {
    type Error = GridError;
    fn try_from(raw: RawGrid) -> Result<Self, GridError> {
        Self::new(raw.source, raw.geometry, raw.fields)
    }
}

impl Grid {
    /// A grid, checked: its geometry stands and every field holds one sample per cell and
    /// component, and as many uncertainties.
    ///
    /// # Errors
    ///
    /// Fails on a geometry that cannot stand or a field of the wrong length.
    pub fn new(
        source: String,
        geometry: GridGeometry,
        fields: Vec<Field>,
    ) -> Result<Self, GridError> {
        geometry.check()?;
        for field in &fields {
            let expected = geometry.cells() * field.components.count();
            let sigma = match &field.uncertainty {
                CellUncertainty::NoneReported => None,
                CellUncertainty::StandardDeviation { sigma, .. } => Some(sigma.len()),
            };
            if let Some(found) = [Some(field.samples.len()), sigma]
                .into_iter()
                .flatten()
                .find(|&n| n != expected)
            {
                return Err(GridError::Samples {
                    field: field.column.clone(),
                    expected,
                    found,
                });
            }
        }
        Ok(Self {
            source,
            geometry,
            fields,
        })
    }

    /// The field read from a column.
    #[must_use]
    pub fn field(&self, column: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.column == column)
    }

    /// One real value of a field at a cell (indices along X, Y, Z), as a quantity with its
    /// uncertainty and kind; `component` is 0 for a scalar. `None` outside the grid, past the
    /// field's components, or for a label field.
    ///
    /// # Errors
    ///
    /// Fails when the stored value or its uncertainty is not finite.
    pub fn value(
        &self,
        field: &Field,
        cell: [usize; 3],
        component: usize,
    ) -> Result<Option<(Quantity, KindOfValue)>, GridError> {
        let (Samples::Real(values), Some(at)) = (&field.samples, self.at(field, cell, component))
        else {
            return Ok(None);
        };
        let uncertainty = match &field.uncertainty {
            CellUncertainty::NoneReported => Uncertainty::NoneReported,
            CellUncertainty::StandardDeviation { sigma, .. } => {
                Uncertainty::Symmetric { sigma: sigma[at] }
            }
        };
        let quantity = Quantity::new(values[at], field.unit, uncertainty)?;
        Ok(Some((quantity, field.kind)))
    }

    /// The label of a segmentation field at a cell (indices along X, Y, Z).
    #[must_use]
    pub fn label(&self, field: &Field, cell: [usize; 3]) -> Option<u32> {
        match &field.samples {
            Samples::Label(labels) => self.at(field, cell, 0).map(|at| labels[at]),
            Samples::Real(_) => None,
        }
    }

    fn at(&self, field: &Field, cell: [usize; 3], component: usize) -> Option<usize> {
        (component < field.components.count())
            .then(|| self.geometry.sample_index(cell))
            .flatten()
            .map(|i| component * self.geometry.cells() + i)
    }
}

#[cfg(test)]
mod tests;
