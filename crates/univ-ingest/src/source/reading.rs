//! How a source's own values are read by people: the form its catalogue writes a designation in,
//! and what each value of a flag column means, each cited from the catalogue's documentation.
//! The release keeps both once for each source, and Univ Server formats and explains with them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Column, Field};

/// The placeholder a designation form holds once, where the source's identifier goes.
pub const IDENTIFIER: &str = "{identifier}";

/// The form a catalogue writes a designation in, such as `HIP {identifier}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignationForm {
    /// The form, holding `{identifier}` exactly once.
    pub form: String,
    /// The catalogue documentation the form is read from, such as a `ReadMe` and its section.
    pub cite: String,
}

impl DesignationForm {
    /// `identifier` written in the form.
    #[must_use]
    pub fn format(&self, identifier: &str) -> String {
        self.form.replacen(IDENTIFIER, identifier, 1)
    }
}

/// What each value of one flag column means, as the catalogue's documentation says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagMeanings {
    /// Each value as the source publishes it, with its meaning.
    pub values: BTreeMap<String, String>,
    /// The catalogue documentation the meanings are read from.
    pub cite: String,
}

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
