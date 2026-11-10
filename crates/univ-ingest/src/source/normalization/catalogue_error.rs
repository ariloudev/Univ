//! An uncertainty a source states once for its whole catalogue, in place of an error column.

use univ_model::units::{Dimension, Unit};

pub use univ_model::release::rows::CatalogueError;

use super::{Problem, RedshiftReading, blank, problem};

/// Checks the catalogue error at `at` of the redshift reading `r`, read from a column in
/// `column`: refused unless the reading is the source's only redshift (`alone`: each value of
/// the source would be taken as having it), beside an error column, on a reading whose kind is
/// given by code (it would reach rows of another kind), in a unit the column cannot take, or
/// scaled by 1 + z on a column not of z.
pub(super) fn check(
    at: &str,
    c: &CatalogueError,
    column: Option<Unit>,
    r: &RedshiftReading,
    alone: bool,
) -> Result<(), Problem> {
    if !alone {
        return Err(problem(
            at,
            "is given on one of several redshifts: each value of the source would be taken as \
             having it",
        ));
    }
    blank(&format!("{at}.basis"), &c.basis)?;
    if r.error.is_some() {
        return Err(problem(at, "is given beside an error column"));
    }
    if r.by_code.is_some() {
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
