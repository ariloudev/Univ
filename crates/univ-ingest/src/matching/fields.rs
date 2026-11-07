//! The field of an entry an identifier is read from, as the matching rules name it.

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;

/// The field of an entry an identifier is read from: `identifier`, `name`, `group_id`, or
/// `{ cross_identifier: PGC }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawEntryField", into = "RawEntryField")]
pub enum EntryField {
    Identifier,
    Name,
    GroupId,
    /// The identifier in another catalogue its source gives in the column of this name.
    CrossIdentifier(String),
}

/// An [`EntryField`] as a rules file writes it: a plain field by its name, a cross-identifier
/// as a map, which YAML reads more plainly than a tagged variant.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum RawEntryField {
    Plain(PlainField),
    Cross { cross_identifier: String },
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PlainField {
    Identifier,
    Name,
    GroupId,
}

impl From<RawEntryField> for EntryField {
    fn from(raw: RawEntryField) -> Self {
        match raw {
            RawEntryField::Plain(PlainField::Identifier) => Self::Identifier,
            RawEntryField::Plain(PlainField::Name) => Self::Name,
            RawEntryField::Plain(PlainField::GroupId) => Self::GroupId,
            RawEntryField::Cross { cross_identifier } => Self::CrossIdentifier(cross_identifier),
        }
    }
}

impl From<EntryField> for RawEntryField {
    fn from(field: EntryField) -> Self {
        match field {
            EntryField::Identifier => Self::Plain(PlainField::Identifier),
            EntryField::Name => Self::Plain(PlainField::Name),
            EntryField::GroupId => Self::Plain(PlainField::GroupId),
            EntryField::CrossIdentifier(cross_identifier) => Self::Cross { cross_identifier },
        }
    }
}

impl EntryField {
    /// The field's text in `entry`, trimmed; none when blank or absent.
    #[must_use]
    pub fn read<'e>(&self, entry: &'e CatalogueEntry) -> Option<&'e str> {
        let text = match self {
            Self::Identifier => Some(&*entry.designation.identifier),
            Self::Name => entry.name.as_deref(),
            Self::GroupId => entry.group_id.as_deref(),
            Self::CrossIdentifier(column) => entry.cross_ids.get(column),
        }?;
        Some(text.trim()).filter(|t| !t.is_empty())
    }
}
