//! Where in a paper a number was read: a quoted sentence, a table cell or a figure.

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

/// Where in which paper a number was read.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Citation {
    /// The id of one of the structure file's papers.
    pub paper: String,
    pub locator: Locator,
}

/// The sentence, table cell or figure a number was read from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Locator {
    /// A sentence, quoted as published, on an optional page.
    Sentence { quote: String, page: Option<u32> },
    /// A cell of a numbered table, named by its row and column, and quoted as printed where
    /// it holds more than the value, such as `11004 ± 59`.
    Table {
        number: String,
        row: String,
        column: String,
        cell: Option<String>,
    },
    /// A numbered figure, and how the number was read from it. A value read from a figure is
    /// always estimated.
    Figure { number: String, how: String },
}

/// A citation as written: exactly one of `sentence`, `table` and `figure`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCitation {
    paper: String,
    #[serde(default)]
    sentence: Option<String>,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    table: Option<String>,
    #[serde(default)]
    row: Option<String>,
    #[serde(default)]
    column: Option<String>,
    #[serde(default)]
    cell: Option<String>,
    #[serde(default)]
    figure: Option<String>,
    #[serde(default)]
    how: Option<String>,
}

impl<'de> Deserialize<'de> for Citation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawCitation::deserialize(deserializer)?;
        Citation::try_from(raw).map_err(de::Error::custom)
    }
}

/// Text that is present and not blank.
fn filled(what: &str, text: Option<String>) -> Result<String, String> {
    match text {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err(format!("needs {what}")),
    }
}

impl TryFrom<RawCitation> for Citation {
    type Error = String;
    fn try_from(raw: RawCitation) -> Result<Self, String> {
        let paper = filled("a paper", Some(raw.paper))?;
        let locator = match (raw.sentence, raw.table, raw.figure) {
            (Some(quote), None, None)
                if raw.row.is_none() && raw.how.is_none() && raw.cell.is_none() =>
            {
                Locator::Sentence {
                    quote: filled("a quoted sentence", Some(quote))?,
                    page: raw.page,
                }
            }
            (None, Some(number), None) if raw.how.is_none() => Locator::Table {
                number: filled("a table number", Some(number))?,
                row: filled("the table row", raw.row)?,
                column: filled("the table column", raw.column)?,
                cell: raw
                    .cell
                    .map(|c| filled("the cell as printed", Some(c)))
                    .transpose()?,
            },
            (None, None, Some(number)) if raw.row.is_none() && raw.cell.is_none() => {
                Locator::Figure {
                    number: filled("a figure number", Some(number))?,
                    how: filled("how the value was read from the figure", raw.how)?,
                }
            }
            _ => {
                return Err(
                    "a citation names exactly one of a sentence, a table (with row and column) \
                     or a figure (with how it was read)"
                        .to_owned(),
                );
            }
        };
        Ok(Self { paper, locator })
    }
}
