//! A star cluster's age: the column it is read from, its error columns and its kind of value.

use serde::{Deserialize, Serialize};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::{Columns, ErrorColumns, Problem, blank, problem};

/// An age published in `value`, in years or as the base-10 logarithm of years, with the
/// columns of its uncertainty in the same unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeReading {
    pub value: String,
    #[serde(default)]
    pub error: Option<ErrorColumns>,
    pub kind_of_value: KindOfValue,
    /// Where the source states how the age was found.
    pub basis: String,
}

/// Checks an age reading, giving the columns it reads: each a column of the definition, the
/// value in years or log years and its error columns in the value's unit, and a credible
/// interval's probability in (0, 1].
pub(super) fn check<'a>(
    table: &Columns<'_>,
    at: &str,
    a: &'a AgeReading,
) -> Result<Vec<&'a str>, Problem> {
    blank(&format!("{at}.basis"), &a.basis)?;
    if let Some(ErrorColumns::CredibleInterval { probability, .. }) = &a.error
        && !(*probability > 0.0 && *probability <= 1.0)
    {
        return Err(problem(at, "gives a probability outside (0, 1]"));
    }
    let mut read = vec![a.value.as_str()];
    read.extend(a.error.iter().flat_map(ErrorColumns::columns));
    for name in &read {
        table.known(at, name)?;
    }
    let unit = table.unit(&a.value);
    if !matches!(unit, Some(Unit::Year | Unit::LogYear)) {
        return Err(problem(
            at,
            format!(
                "reads {}, which is neither in years nor in log years",
                a.value
            ),
        ));
    }
    if let Some(other) = read.iter().find(|name| table.unit(name) != unit) {
        return Err(problem(
            at,
            format!("reads {other} in another unit than {}", a.value),
        ));
    }
    Ok(read)
}
