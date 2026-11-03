//! Cited values: a number with its unit, kind of value, typed uncertainty and the place in a paper
//! it was read from.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use super::citation::{Citation, Locator};
use super::numbers::{consistent, contains_whole, has_magnitude, has_number, published};

/// A number read from a paper: its quantity, kind of value and citation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cited {
    pub quantity: Quantity,
    pub kind: KindOfValue,
    pub cite: Citation,
    /// The number as the paper prints it, such as `14h50m` or `03h19m`, where that differs from
    /// `value`; it must stand for `value` within the rounding of its last digit, and a quoted
    /// sentence must contain it.
    pub as_published: Option<String>,
    /// Where the paper gives the uncertainty, when not in the value's own citation.
    pub uncertainty_cite: Option<Box<Citation>>,
    /// The cosmology or Hubble constant the paper used to give the number, such as
    /// `H0 = 50 km/s/Mpc` for a length in Mpc.
    pub assumes: Option<Box<Assumes>>,
    pub note: Option<String>,
}

/// The cosmology a number depends on: as stated in the file, or cited from the paper, whose
/// quoted sentence then holds every number of `cosmology`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Assumes {
    Stated(String),
    Cited { cosmology: String, cite: Citation },
}

impl Assumes {
    /// The cosmology in words.
    #[must_use]
    pub fn cosmology(&self) -> &str {
        match self {
            Self::Stated(text)
            | Self::Cited {
                cosmology: text, ..
            } => text,
        }
    }
}

/// A cited value as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawCited {
    pub value: f64,
    pub unit: Unit,
    pub kind: KindOfValue,
    pub uncertainty: Uncertainty,
    #[serde(default)]
    pub cite: Option<Citation>,
    #[serde(default)]
    pub as_published: Option<String>,
    #[serde(default)]
    pub uncertainty_cite: Option<Citation>,
    #[serde(default)]
    pub assumes: Option<Assumes>,
    #[serde(default)]
    pub note: Option<String>,
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
            ("as_published", raw.as_published.as_deref()),
            ("assumes", raw.assumes.as_ref().map(Assumes::cosmology)),
            ("note", raw.note.as_deref()),
        ] {
            if text.is_some_and(|t| t.trim().is_empty()) {
                return Err(format!("{what} is empty"));
            }
        }
        if let Some(text) = &raw.as_published {
            let Some(p) = published(text) else {
                return Err(format!(
                    "as_published {text:?} cannot be read as a number, hours or degrees"
                ));
            };
            if !consistent(raw.value, raw.unit, &p) {
                return Err(format!(
                    "as_published {text:?} is not the value {} {:?} within the rounding of its \
                     last digit",
                    raw.value, raw.unit
                ));
            }
        }
        Ok(Self {
            quantity,
            kind: raw.kind,
            cite,
            as_published: raw.as_published,
            uncertainty_cite: raw.uncertainty_cite.map(Box::new),
            assumes: raw.assumes.map(Box::new),
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
    /// the value as a whole number token, such as `16,000` for 16000 but not `1650` for 165.
    #[must_use]
    pub fn appears_in(&self, quote: &str) -> bool {
        match &self.as_published {
            Some(published) => contains_whole(quote, published),
            None => has_number(quote, self.value()),
        }
    }

    /// The numbers of the uncertainty: an error, or the two sides of an interval.
    #[must_use]
    pub fn uncertainty_numbers(&self) -> Vec<f64> {
        match self.quantity.uncertainty {
            Uncertainty::Symmetric { sigma } => vec![sigma],
            Uncertainty::Asymmetric { minus, plus }
            | Uncertainty::CredibleInterval { minus, plus, .. } => vec![minus, plus],
            _ => vec![],
        }
    }

    /// The first number of the uncertainty that its citation does not show: in the quoted
    /// sentence as a whole token, in the table cell, row or column as printed, or in how it was
    /// read from the figure. The uncertainty's own citation, when it has one, is the one read.
    #[must_use]
    pub fn uncertainty_not_shown(&self) -> Option<f64> {
        let cite = self.uncertainty_cite.as_deref().unwrap_or(&self.cite);
        let texts: Vec<&str> = match &cite.locator {
            Locator::Sentence { quote, .. } => vec![quote],
            Locator::Table {
                row, column, cell, ..
            } => [Some(row), Some(column), cell.as_ref()]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect(),
            Locator::Figure { how, .. } => vec![how],
        };
        self.uncertainty_numbers()
            .into_iter()
            .find(|x| !texts.iter().any(|t| has_magnitude(t, *x)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_found_in_its_sentence_as_a_whole_token() {
        let cited = |value: f64| Cited {
            quantity: Quantity::new(value, Unit::Dimensionless, Uncertainty::NoneReported)
                .unwrap_or_else(|e| panic!("{e}")),
            kind: KindOfValue::Measured,
            cite: Citation {
                paper: "p".to_owned(),
                locator: Locator::Figure {
                    number: "1".to_owned(),
                    how: "h".to_owned(),
                },
            },
            as_published: None,
            uncertainty_cite: None,
            assumes: None,
            note: None,
        };
        assert!(cited(16000.0).appears_in("at 16,000 km s-1"));
        assert!(cited(-1200.0).appears_in("at \u{2212}1,200"));
        assert!(!cited(165.0).appears_in("at 1650 km s-1"));
    }
}
