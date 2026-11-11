//! The check of a colour index reading.

use univ_model::units::Unit;

use super::{ColourReading, Columns, Problem, problem};

/// The photometric bands a colour index may lie between: Johnson–Cousins `U`, `B`, `V`, `R`
/// and `I`, Gaia's `G`, `BP` and `RP`, and 2MASS's `J`, `H` and `Ks`, as the catalogues name
/// them. A band outside them is refused, so a misspelling cannot leave a colour unread.
const BANDS: [&str; 11] = ["U", "B", "V", "R", "I", "G", "BP", "RP", "J", "H", "Ks"];

/// Checks a colour reading, giving the columns it names, each a column of the definition in
/// magnitudes.
pub(super) fn check<'c>(
    table: &Columns<'_>,
    at: &str,
    c: &'c ColourReading,
) -> Result<Vec<&'c str>, Problem> {
    if let Some(band) = c.bands.iter().find(|b| !BANDS.contains(&b.as_str())) {
        return Err(problem(
            at,
            format!(
                "names the band {band}, which is not one of {}",
                BANDS.join(", ")
            ),
        ));
    }
    if c.bands[0] == c.bands[1] {
        return Err(problem(at, format!("names the band {} twice", c.bands[0])));
    }
    let names: Vec<&str> = c
        .column
        .iter()
        .chain(&c.error)
        .chain(c.from.iter().flatten())
        .map(String::as_str)
        .collect();
    for name in &names {
        table.known(at, name)?;
        if table.unit(name) != Some(Unit::Magnitude) {
            return Err(problem(
                at,
                format!("names {name}, which is not in magnitudes"),
            ));
        }
    }
    Ok(names)
}
