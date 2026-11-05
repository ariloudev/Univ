//! The lowest redshift a source's rows may take: below it, a row is not taken, such as a star
//! a galaxy survey measured at z = 0.

use serde::{Deserialize, Serialize};

use super::{Problem, blank, problem};

/// Rows whose redshift lies below `below`, in the unit of its column, are left out as
/// unsuitable redshifts, for the reason `basis` gives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedshiftFloor {
    pub below: f64,
    pub basis: String,
}

/// Checks the floor at `at`: a finite number with a basis.
pub(super) fn check(at: &str, f: &RedshiftFloor) -> Result<(), Problem> {
    blank(&format!("{at}.basis"), &f.basis)?;
    if !f.below.is_finite() {
        return Err(problem(format!("{at}.below"), "is not a finite number"));
    }
    Ok(())
}
