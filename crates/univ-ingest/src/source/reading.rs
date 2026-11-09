//! How a source's own values are read by people: the form its catalogue writes a designation in,
//! and what each value of a flag column means, each cited from the catalogue's documentation.
//! The release keeps both once for each source, and Univ Server formats and explains with them.

use std::collections::BTreeMap;

use univ_model::api::sources::{DesignationForm, FlagMeanings, IDENTIFIER};

use super::{Column, Field};

/// The checks serde cannot make of a definition's designation form and flag meanings, as the
/// field and the problem.
pub(super) fn check(
    designation: Option<&DesignationForm>,
    flags: &BTreeMap<String, FlagMeanings>,
    columns: &[Column],
) -> Result<(), (String, String)> {
    if let Some(d) = designation {
        let held = d.form.matches(IDENTIFIER).count();
        if held != 1 {
            return Err((
                "designation.form".to_owned(),
                format!("{} holds {IDENTIFIER} {held} times, not once", d.form),
            ));
        }
        if d.form.replacen(IDENTIFIER, "", 1).trim().is_empty() {
            return Err((
                "designation.form".to_owned(),
                format!(
                    "{} holds nothing but {IDENTIFIER}: it reads every name as a designation",
                    d.form
                ),
            ));
        }
        if d.cite.trim().is_empty() {
            return Err(("designation.cite".to_owned(), "is empty".to_owned()));
        }
    }
    for (column, meanings) in flags {
        let at = format!("flags.{column}");
        let taken = columns
            .iter()
            .any(|c| &c.column == column && c.field == Field::Quality);
        if !taken {
            return Err((
                at,
                "names no column the definition takes as a flag (field quality)".to_owned(),
            ));
        }
        if meanings.cite.trim().is_empty() {
            return Err((format!("{at}.cite"), "is empty".to_owned()));
        }
        if meanings.values.is_empty() {
            return Err((format!("{at}.values"), "gives no value".to_owned()));
        }
        if let Some((value, _)) = meanings.values.iter().find(|(_, m)| m.trim().is_empty()) {
            return Err((format!("{at}.values.{value}"), "is empty".to_owned()));
        }
    }
    Ok(())
}
