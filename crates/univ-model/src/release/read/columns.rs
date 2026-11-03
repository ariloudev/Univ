//! Typed reads of one D1 row's columns: integers read leniently from whole floats, as D1 gives
//! JavaScript numbers; terms through the release's vocabulary; uncertainties through
//! [`UncertaintyColumns`]. Every failure names the table, the column and the value.

use serde::de::value::Error as PlainError;
use serde::de::{DeserializeOwned, IntoDeserializer};
use serde_json::Value;

use super::{ReadError, Row, Vocabulary};
use crate::release::UncertaintyColumns;
use crate::units::Unit;
use crate::value::{Quantity, Uncertainty};

/// The value a term names, as univ-model serializes it: a unit variant's name, read from the
/// term in place, or the JSON of another value, such as a mixed distance method.
pub(super) fn decode_term<T: DeserializeOwned>(term: &str) -> Option<T> {
    if term.starts_with('{') || term.starts_with('[') {
        serde_json::from_str(term).ok()
    } else {
        T::deserialize(IntoDeserializer::<PlainError>::into_deserializer(term)).ok()
    }
}

/// One row of `table`, read with the release's vocabulary.
pub(super) struct Columns<'a> {
    pub v: &'a Vocabulary,
    pub table: &'static str,
    pub row: &'a Row,
}

impl<'a> Columns<'a> {
    pub fn new(v: &'a Vocabulary, table: &'static str, row: &'a Row) -> Self {
        Self { v, table, row }
    }

    /// A failure of column `column`.
    pub fn error(&self, column: &str, what: impl std::fmt::Display) -> ReadError {
        ReadError(format!(
            "release {} {}.{column}: {what}",
            self.v.release(),
            self.table
        ))
    }

    /// The column's value, none when NULL.
    fn get(&self, column: &str) -> Result<Option<&'a Value>, ReadError> {
        match self.row.get(column) {
            None => Err(self.error(column, "the row has no such column")),
            Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(value)),
        }
    }

    /// A column that must not be NULL.
    fn required<T>(&self, column: &str, value: Option<T>) -> Result<T, ReadError> {
        value.ok_or_else(|| self.error(column, "is NULL"))
    }

    pub fn int_opt(&self, column: &str) -> Result<Option<i64>, ReadError> {
        let Some(value) = self.get(column)? else {
            return Ok(None);
        };
        if let Some(i) = value.as_i64() {
            return Ok(Some(i));
        }
        match value.as_f64() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a whole number within the range of i64, checked first"
            )]
            Some(x) if x.fract() == 0.0 && x.abs() < 9.2e18 => Ok(Some(x as i64)),
            _ => Err(self.error(column, format!("{value} is not an integer"))),
        }
    }

    pub fn int(&self, column: &str) -> Result<i64, ReadError> {
        let i = self.int_opt(column)?;
        self.required(column, i)
    }

    /// A non-negative integer that fits `u32`, such as an entry's number.
    pub fn small_opt(&self, column: &str) -> Result<Option<u32>, ReadError> {
        self.int_opt(column)?
            .map(|i| {
                u32::try_from(i).map_err(|_| self.error(column, format!("{i} is out of range")))
            })
            .transpose()
    }

    pub fn small(&self, column: &str) -> Result<u32, ReadError> {
        let i = self.small_opt(column)?;
        self.required(column, i)
    }

    pub fn real_opt(&self, column: &str) -> Result<Option<f64>, ReadError> {
        self.get(column)?
            .map(|value| {
                value
                    .as_f64()
                    .ok_or_else(|| self.error(column, format!("{value} is not a number")))
            })
            .transpose()
    }

    pub fn real(&self, column: &str) -> Result<f64, ReadError> {
        let x = self.real_opt(column)?;
        self.required(column, x)
    }

    pub fn text_opt(&self, column: &str) -> Result<Option<&'a str>, ReadError> {
        self.get(column)?
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| self.error(column, format!("{value} is not text")))
            })
            .transpose()
    }

    pub fn text(&self, column: &str) -> Result<&'a str, ReadError> {
        let s = self.text_opt(column)?;
        self.required(column, s)
    }

    /// A JSON column, none when NULL.
    pub fn json_opt<T: DeserializeOwned>(&self, column: &str) -> Result<Option<T>, ReadError> {
        self.text_opt(column)?
            .map(|text| {
                serde_json::from_str(text)
                    .map_err(|e| self.error(column, format!("{text:?} does not read: {e}")))
            })
            .transpose()
    }

    /// The term a term column names, none when NULL.
    pub fn term_opt(&self, column: &str) -> Result<Option<&'a str>, ReadError> {
        let v = self.v;
        self.int_opt(column)?
            .map(|id| {
                v.term(id).ok_or_else(|| {
                    self.error(
                        column,
                        format!("term {id} is not among the release's terms"),
                    )
                })
            })
            .transpose()
    }

    pub fn term(&self, column: &str) -> Result<&'a str, ReadError> {
        let t = self.term_opt(column)?;
        self.required(column, t)
    }

    /// The value of type `T` a term column names, none when NULL.
    pub fn typed_opt<T: DeserializeOwned>(&self, column: &str) -> Result<Option<T>, ReadError> {
        self.term_opt(column)?
            .map(|term| {
                decode_term(term).ok_or_else(|| {
                    self.error(
                        column,
                        format!("the term {term:?} is not a {}", std::any::type_name::<T>()),
                    )
                })
            })
            .transpose()
    }

    pub fn typed<T: DeserializeOwned>(&self, column: &str) -> Result<T, ReadError> {
        let t = self.typed_opt(column)?;
        self.required(column, t)
    }

    /// The uncertainty of columns `{prefix}form`, `minus`, `plus` and `level`; none when its
    /// form is NULL.
    pub fn uncertainty(&self, prefix: &str) -> Result<Option<Uncertainty>, ReadError> {
        let column = format!("{prefix}form");
        let Some(term) = self.term_opt(&column)? else {
            return Ok(None);
        };
        let columns = UncertaintyColumns {
            form: term,
            minus: self.real_opt(&format!("{prefix}minus"))?,
            plus: self.real_opt(&format!("{prefix}plus"))?,
            level: self.real_opt(&format!("{prefix}level"))?,
        };
        columns.uncertainty().map(Some).ok_or_else(|| {
            self.error(
                &column,
                format!("{term:?} is no uncertainty form, or lacks a number its form takes"),
            )
        })
    }

    /// The quantity of `value` in `unit` with the uncertainty of `prefix`, all three required.
    pub fn quantity(&self, value: f64, unit: Unit, prefix: &str) -> Result<Quantity, ReadError> {
        let uncertainty = self.uncertainty(prefix)?;
        let uncertainty = self.required(&format!("{prefix}form"), uncertainty)?;
        Quantity::new(value, unit, uncertainty).map_err(|e| self.error(&format!("{prefix}form"), e))
    }
}
