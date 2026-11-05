//! An uncertainty a source states once for its whole catalogue, in place of an error column.

use univ_model::units::{Dimension, Unit};

pub use univ_model::release::rows::CatalogueError;

use super::{Problem, blank, problem};

/// Checks the catalogue error at `at` of a redshift read from a column in `column`: refused
/// beside an error column, on a reading whose kind is given by code (it would reach rows of
/// another kind), in a unit the column cannot take, or scaled by 1 + z on a column not of z.
pub(super) fn check(
    at: &str,
    c: &CatalogueError,
    column: Option<Unit>,
    has_error_column: bool,
    by_code: bool,
) -> Result<(), Problem> {
    blank(&format!("{at}.basis"), &c.basis)?;
    if has_error_column {
        return Err(problem(at, "is given beside an error column"));
    }
    if by_code {
        return Err(problem(
            at,
            "is given on a redshift whose kind is given by code, so it would reach rows of \
             another kind",
        ));
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
