//! Cited values: a number with its unit, kind of value, typed uncertainty and the place in a paper
//! it was read from.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

/// A number read from a paper: its quantity, kind of value and citation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cited {
    pub quantity: Quantity,
    pub kind: KindOfValue,
    pub cite: Citation,
    /// The number as the paper prints it, such as `14h50m` or `2x10^5`, where that differs from
    /// `value`; a quoted sentence must contain it.
    pub as_published: Option<String>,
    /// The cosmology or Hubble constant the paper used to give the number, such as
    /// `H0 = 50 km/s/Mpc` for a length in Mpc.
    pub assumes: Option<String>,
    pub note: Option<String>,
}

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
    /// A cell of a numbered table, named by its row and column.
    Table {
        number: String,
        row: String,
        column: String,
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
            (Some(quote), None, None) if raw.row.is_none() && raw.how.is_none() => {
                Locator::Sentence {
                    quote: filled("a quoted sentence", Some(quote))?,
                    page: raw.page,
                }
            }
            (None, Some(number), None) if raw.how.is_none() => Locator::Table {
                number: filled("a table number", Some(number))?,
                row: filled("the table row", raw.row)?,
                column: filled("the table column", raw.column)?,
            },
            (None, None, Some(number)) if raw.row.is_none() => Locator::Figure {
                number: filled("a figure number", Some(number))?,
                how: filled("how the value was read from the figure", raw.how)?,
            },
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

/// A cited value as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCited {
    value: f64,
    unit: Unit,
    kind: KindOfValue,
    uncertainty: Uncertainty,
    #[serde(default)]
    cite: Option<Citation>,
    #[serde(default)]
    as_published: Option<String>,
    #[serde(default)]
    assumes: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

impl TryFrom<RawCited> for Cited {
    type Error = String;
    fn try_from(raw: RawCited) -> Result<Self, String> {
        let Some(cite) = raw.cite else {
            return Err(uncited(raw.value));
        };
        let quantity =
            Quantity::new(raw.value, raw.unit, raw.uncertainty).map_err(|e| e.to_string())?;
        if matches!(cite.locator, Locator::Figure { .. }) && raw.kind != KindOfValue::Estimated {
            return Err(format!(
                "the number {} is read from figure {} and so is estimated, not {:?}",
                raw.value,
                figure_number(&cite),
                raw.kind
            ));
        }
        for (what, text) in [
            ("as_published", &raw.as_published),
            ("assumes", &raw.assumes),
            ("note", &raw.note),
        ] {
            if text.as_ref().is_some_and(|t| t.trim().is_empty()) {
                return Err(format!("{what} is empty"));
            }
        }
        Ok(Self {
            quantity,
            kind: raw.kind,
            cite,
            as_published: raw.as_published,
            assumes: raw.assumes,
            note: raw.note,
        })
    }
}

fn figure_number(cite: &Citation) -> &str {
    match &cite.locator {
        Locator::Figure { number, .. } => number,
        _ => "",
    }
}

/// The refusal for a number that cites nothing.
pub(crate) fn uncited(number: f64) -> String {
    format!(
        "the number {number} cites nothing: give it a value, unit, kind, uncertainty and a cite \
         naming the paper and the sentence, table or figure"
    )
}

/// Reads a cited value, and names a bare number as uncited rather than as a type mismatch.
struct CitedVisitor;

impl<'de> Visitor<'de> for CitedVisitor {
    type Value = Cited;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a cited value: value, unit, kind, uncertainty and cite")
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "only echoed in the refusal message"
    )]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Cited, E> {
        Err(E::custom(uncited(v as f64)))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "only echoed in the refusal message"
    )]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Cited, E> {
        Err(E::custom(uncited(v as f64)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Cited, E> {
        Err(E::custom(uncited(v)))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Cited, A::Error> {
        let raw = RawCited::deserialize(de::value::MapAccessDeserializer::new(map))?;
        Cited::try_from(raw).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Cited {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(CitedVisitor)
    }
}

impl Cited {
    #[must_use]
    pub fn value(&self) -> f64 {
        self.quantity.value
    }

    #[must_use]
    pub fn unit(&self) -> Unit {
        self.quantity.unit
    }

    /// Whether a quoted sentence holds the number as published: `as_published` when given, else
    /// the value written plainly or with thousands separators.
    #[must_use]
    pub fn appears_in(&self, quote: &str) -> bool {
        let quote = quote.replace('\u{2212}', "-");
        if let Some(published) = &self.as_published {
            return quote.contains(published.as_str());
        }
        renderings(self.value()).iter().any(|r| quote.contains(r))
    }
}

/// The ways a sentence may print `value`: plainly, and with comma or thin-space thousands
/// separators.
fn renderings(value: f64) -> Vec<String> {
    let plain = format!("{value}");
    let (sign, digits) = plain
        .strip_prefix('-')
        .map_or(("", plain.as_str()), |d| ("-", d));
    let (whole, fraction) = digits
        .split_once('.')
        .map_or((digits, None), |(w, f)| (w, Some(f)));
    let mut out = vec![plain.clone()];
    if whole.len() > 3 {
        for separator in [",", " ", "\u{2009}"] {
            let mut grouped = String::new();
            for (i, c) in whole.chars().enumerate() {
                if i > 0 && (whole.len() - i) % 3 == 0 {
                    grouped.push_str(separator);
                }
                grouped.push(c);
            }
            let fraction = fraction.map(|f| format!(".{f}")).unwrap_or_default();
            out.push(format!("{sign}{grouped}{fraction}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_found_in_its_sentence_with_or_without_separators() {
        assert_eq!(renderings(16000.0)[1], "16,000");
        assert_eq!(renderings(-1200.0)[1], "-1,200");
        assert_eq!(renderings(0.22), vec!["0.22".to_owned()]);
    }
}
