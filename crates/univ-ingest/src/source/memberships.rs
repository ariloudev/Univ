//! A source's own assignment of its rows to researched structures: a flag column whose values
//! name structures, such as Reid et al. 2019's `Arm` column naming the spiral arm each
//! star-forming region lies on. The release records each object so assigned as a member of the
//! structure (`memberships`), keeping the source and the value as published.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use univ_model::api::sources::FlagMeanings;

use super::{Column, Field, SourceDefinition};

/// A flag column whose values assign a row to a structure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Membership {
    /// The flag column, which the definition takes as a quality field and explains in `flags`.
    pub column: String,
    /// Where the source makes the assignment, and where the structures are defined, cited.
    pub cite: String,
    /// The structure each value names, by its id; a value not listed names none.
    pub structures: BTreeMap<String, String>,
}

impl Membership {
    /// The structure that `value` of the column names, if any.
    #[must_use]
    pub fn structure(&self, value: &str) -> Option<&str> {
        self.structures.get(value).map(String::as_str)
    }
}

/// The checks serde cannot make of a definition's memberships, as the field and the problem:
/// each names a flag column the definition takes and explains, is cited, and maps only values
/// its flag meanings give.
pub(super) fn check(
    memberships: &[Membership],
    flags: &BTreeMap<String, FlagMeanings>,
    columns: &[Column],
) -> Result<(), (String, String)> {
    let mut seen = BTreeSet::new();
    for (i, m) in memberships.iter().enumerate() {
        let at = format!("memberships[{i}]");
        let taken = columns
            .iter()
            .any(|c| c.column == m.column && c.field == Field::Quality);
        if !taken {
            return Err((
                format!("{at}.column"),
                format!(
                    "{:?} is no column the definition takes as a flag (field quality)",
                    m.column
                ),
            ));
        }
        if !seen.insert(m.column.as_str()) {
            return Err((
                format!("{at}.column"),
                format!("{:?} is given twice", m.column),
            ));
        }
        let Some(meanings) = flags.get(&m.column) else {
            return Err((
                format!("{at}.column"),
                format!(
                    "{:?} has no meanings under flags, which cite what its values mean",
                    m.column
                ),
            ));
        };
        if m.cite.trim().is_empty() {
            return Err((format!("{at}.cite"), "is empty".to_owned()));
        }
        if m.structures.is_empty() {
            return Err((format!("{at}.structures"), "maps no value".to_owned()));
        }
        for (value, structure) in &m.structures {
            if !meanings.values.contains_key(value) {
                return Err((
                    format!("{at}.structures.{value}"),
                    format!("is no value flags.{} gives", m.column),
                ));
            }
            if structure.trim().is_empty() {
                return Err((format!("{at}.structures.{value}"), "is empty".to_owned()));
            }
        }
    }
    Ok(())
}

/// Refuses a membership of `definitions` that names a structure not among `structures`, the
/// ids of the release's researched structures.
///
/// # Errors
///
/// Names the source, its column, the value and the structure it names.
pub fn check_structures<'s>(
    definitions: &[SourceDefinition],
    structures: impl IntoIterator<Item = &'s str>,
) -> Result<(), String> {
    let known: BTreeSet<&str> = structures.into_iter().collect();
    for d in definitions {
        for m in &d.memberships {
            if let Some((value, id)) = m
                .structures
                .iter()
                .find(|(_, id)| !known.contains(id.as_str()))
            {
                return Err(format!(
                    "source {}: memberships of column {}: value {value:?} names structure \
                     {id:?}, which is none of the release's structures",
                    d.id, m.column
                ));
            }
        }
    }
    Ok(())
}
