//! An uncertainty a source states once for its whole catalogue, in place of an error column.

use serde::{Deserialize, Serialize};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::units::{Dimension, Unit};

use super::{Problem, blank, problem};

/// An uncertainty a source states once for its whole catalogue, such as 2dFGRS's rms of 85 km/s
/// or a photometric redshift scatter σz / (1 + z): given to each value as a symmetric error,
/// estimated, citing `basis`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueError {
    pub value: f64,
    /// Its unit: the column's, or km/s for a column of z (σz = σv / c).
    pub unit: Unit,
    /// Whether it is stated per unit of (1 + z), as σz / (1 + z) is.
    #[serde(default)]
    pub times_one_plus_z: bool,
    /// Where the source states it, quoted or cited.
    pub basis: String,
}

impl CatalogueError {
    /// The error in the unit of a column in `column`, before any factor of (1 + z): none when
    /// the units cannot meet, which [`check`] refuses.
    #[must_use]
    pub fn in_unit(&self, column: Unit) -> Option<f64> {
        if column.dimension() == Dimension::Dimensionless
            && self.unit.dimension() == Dimension::Velocity
        {
            let km_s = self.unit.factor_to(Unit::KilometrePerSecond).ok()?;
            return Some(self.value * km_s / SPEED_OF_LIGHT);
        }
        Some(self.value * self.unit.factor_to(column).ok()?)
    }
}

/// Checks the catalogue error at `at` of a redshift read from a column in `column`, refusing it
/// beside an error column.
pub(super) fn check(
    at: &str,
    c: &CatalogueError,
    column: Option<Unit>,
    has_error_column: bool,
) -> Result<(), Problem> {
    blank(&format!("{at}.basis"), &c.basis)?;
    if has_error_column {
        return Err(problem(at, "is given beside an error column"));
    }
    if !(c.value.is_finite() && c.value > 0.0) {
        return Err(problem(at, "is not a positive number"));
    }
    if column.and_then(|u| c.in_unit(u)).is_none() {
        return Err(problem(at, "is in a unit that cannot be the column's"));
    }
    if c.times_one_plus_z && column.map(Unit::dimension) != Some(Dimension::Dimensionless) {
        return Err(problem(at, "scales with 1 + z only on a column of z"));
    }
    Ok(())
}
