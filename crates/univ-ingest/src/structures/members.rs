//! A structure's members as a paper tabulates them, or as they were digitised from a figure. Each
//! number cites the table (or figure), its row by the member's name and its column by its header.

use serde::{Deserialize, Serialize};
use univ_model::units::{Dimension, Unit};
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use super::cited::{Assumes, uncited};
use univ_model::position::Frame;

/// The members of a structure, with every column's unit, kind of value and uncertainty.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawMembers")]
pub struct Members {
    pub frame: Frame,
    /// What each member is, such as `quasar`, `galaxy cluster` or `Mg II absorber`.
    pub tracer: String,
    pub source: MemberSource,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

/// The table or figure the members were read from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum MemberSource {
    Table {
        paper: String,
        number: String,
    },
    /// Digitised from a figure: every number is then estimated.
    Figure {
        paper: String,
        number: String,
        how: String,
    },
}

impl MemberSource {
    #[must_use]
    pub fn paper(&self) -> &str {
        match self {
            Self::Table { paper, .. } | Self::Figure { paper, .. } => paper,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSource {
    paper: String,
    #[serde(default)]
    table: Option<String>,
    #[serde(default)]
    figure: Option<String>,
    #[serde(default)]
    how: Option<String>,
}

impl<'de> Deserialize<'de> for MemberSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawSource::deserialize(deserializer)?;
        let blank = |t: &Option<String>| t.as_ref().is_none_or(|t| t.trim().is_empty());
        match (raw.table, raw.figure) {
            (Some(number), None) if raw.how.is_none() && !number.trim().is_empty() => {
                Ok(Self::Table {
                    paper: raw.paper,
                    number,
                })
            }
            (None, Some(number)) if !blank(&raw.how) && !number.trim().is_empty() => {
                Ok(Self::Figure {
                    paper: raw.paper,
                    number,
                    how: raw.how.unwrap_or_default(),
                })
            }
            _ => Err(serde::de::Error::custom(
                "members come from a table, or from a figure with how they were read",
            )),
        }
    }
}

/// What a column holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnName {
    Lon,
    Lat,
    Redshift,
    Velocity,
    Distance,
    /// A Cartesian coordinate along the table's frame's first, second or third axis, such as
    /// supergalactic SGX, SGY and SGZ: with the other two, it gives the member's direction.
    X,
    Y,
    Z,
    /// How many published entries one digitised point stands for, where points overlap.
    Count,
}

/// How a column's cells are written: as numbers, or as sexagesimal text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    #[default]
    Decimal,
    /// Hours, minutes and seconds of right ascension, such as `10:41:39.15`, read as degrees.
    Hms,
    /// Degrees, arcminutes and arcseconds, such as `+14:35:30.2`.
    Dms,
}

/// A column of the member table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: ColumnName,
    /// The column's heading in the paper's table, or the axis of the figure.
    pub header: String,
    pub unit: Unit,
    #[serde(default)]
    pub format: Format,
    pub kind: KindOfValue,
    pub uncertainty: Uncertainty,
    /// The cosmology the paper used for a column of lengths, as for a cited value.
    #[serde(default)]
    pub assumes: Option<Assumes>,
}

/// A member: its name in the paper and its values, one per column, in the column's unit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
    pub name: String,
    pub values: Vec<f64>,
}

/// A table cell as written.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum Cell {
    Number(f64),
    Text(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMembers {
    frame: Frame,
    tracer: String,
    #[serde(default)]
    source: Option<MemberSource>,
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
}

impl TryFrom<RawMembers> for Members {
    type Error = String;
    fn try_from(raw: RawMembers) -> Result<Self, String> {
        let Some(source) = raw.source else {
            let first = raw.rows.iter().flatten().find_map(|c| match c {
                Cell::Number(n) => Some(*n),
                Cell::Text(_) => None,
            });
            return Err(match first {
                Some(n) => format!(
                    "the member table has no source, so {}",
                    uncited(n).replace("the number", "its first number")
                ),
                None => "the member table has no source".to_owned(),
            });
        };
        check_columns(&raw.columns, &source)?;
        if raw.tracer.trim().is_empty() || raw.rows.is_empty() {
            return Err("a member table names its tracer and lists members".into());
        }
        let mut rows: Vec<Row> = Vec::with_capacity(raw.rows.len());
        for (i, cells) in raw.rows.into_iter().enumerate() {
            let row = read_row(i, cells, &raw.columns)?;
            if rows.iter().any(|r| r.name == row.name) {
                return Err(format!("rows[{i}] repeats the member {}", row.name));
            }
            rows.push(row);
        }
        Ok(Self {
            frame: raw.frame,
            tracer: raw.tracer,
            source,
            columns: raw.columns,
            rows,
        })
    }
}

fn check_columns(columns: &[Column], source: &MemberSource) -> Result<(), String> {
    let one = |names: &[ColumnName]| {
        names
            .iter()
            .all(|&name| columns.iter().filter(|c| c.name == name).count() == 1)
    };
    let none = |names: &[ColumnName]| !columns.iter().any(|c| names.contains(&c.name));
    let (sky, cartesian) = (
        [ColumnName::Lon, ColumnName::Lat],
        [ColumnName::X, ColumnName::Y, ColumnName::Z],
    );
    if !(one(&sky) && none(&cartesian) || one(&cartesian) && none(&sky)) {
        return Err(
            "the member table needs one lon and one lat column, or one each of x, y and z"
                .to_owned(),
        );
    }
    // Cartesian coordinates give a member's direction only: its redshift places it.
    if one(&cartesian) {
        let units: Vec<_> = columns
            .iter()
            .filter(|c| cartesian.contains(&c.name))
            .map(|c| c.unit)
            .collect();
        if units.windows(2).any(|w| w[0] != w[1]) {
            return Err("the x, y and z columns are in one unit".to_owned());
        }
        if none(&[ColumnName::Redshift, ColumnName::Velocity]) {
            return Err(
                "a member table of x, y and z gives each member's redshift or velocity".to_owned(),
            );
        }
    }
    for (i, c) in columns.iter().enumerate() {
        let dimension = c.unit.dimension();
        let fits = match c.name {
            ColumnName::Lon | ColumnName::Lat => dimension == Dimension::Angle,
            ColumnName::Redshift | ColumnName::Count => c.unit == Unit::Dimensionless,
            ColumnName::Velocity => dimension == Dimension::Velocity,
            ColumnName::Distance | ColumnName::X | ColumnName::Y | ColumnName::Z => {
                matches!(dimension, Dimension::Length | Dimension::HubbleScaledLength)
            }
        };
        let sexagesimal = c.format != Format::Decimal;
        if !fits || (sexagesimal && c.unit != Unit::Degree) || c.header.trim().is_empty() {
            return Err(format!(
                "columns[{i}]: a {:?} column cannot be in {:?} written {:?}, and needs a header",
                c.name, c.unit, c.format
            ));
        }
        if matches!(source, MemberSource::Figure { .. }) && c.kind != KindOfValue::Estimated {
            return Err(format!(
                "columns[{i}]: values read from a figure are estimated, not {:?}",
                c.kind
            ));
        }
        Quantity::new(0.0, c.unit, c.uncertainty).map_err(|e| format!("columns[{i}]: {e}"))?;
    }
    Ok(())
}

fn read_row(i: usize, cells: Vec<Cell>, columns: &[Column]) -> Result<Row, String> {
    let mut cells = cells.into_iter();
    let Some(Cell::Text(name)) = cells.next() else {
        return Err(format!("rows[{i}] starts with the member's name"));
    };
    let cells: Vec<Cell> = cells.collect();
    if cells.len() != columns.len() {
        return Err(format!(
            "rows[{i}] ({name}) has {} values for {} columns",
            cells.len(),
            columns.len()
        ));
    }
    let values = cells
        .iter()
        .zip(columns)
        .map(|(cell, column)| read_cell(cell, column.format))
        .collect::<Option<Vec<f64>>>()
        .filter(|v| v.iter().all(|x| x.is_finite()))
        .ok_or_else(|| format!("rows[{i}] ({name}) holds a value its column cannot read"))?;
    Ok(Row { name, values })
}

fn read_cell(cell: &Cell, format: Format) -> Option<f64> {
    match (cell, format) {
        (Cell::Number(n), Format::Decimal) => Some(*n),
        (Cell::Text(t), Format::Hms) => sexagesimal(t).map(|h| h * 15.0),
        (Cell::Text(t), Format::Dms) => sexagesimal(t),
        _ => None,
    }
}

/// `[+-]a:b:c` (or with spaces) as `a + b/60 + c/3600`, keeping the sign of the whole.
fn sexagesimal(text: &str) -> Option<f64> {
    let text = text.trim();
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let parts: Vec<f64> = rest
        .split([':', ' '])
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [a, b, c] = parts[..] else { return None };
    if !(0.0..60.0).contains(&b) || !(0.0..60.0).contains(&c) {
        return None;
    }
    let value = a + b / 60.0 + c / 3600.0;
    Some(if negative { -value } else { value })
}

impl Members {
    /// The index of the column holding `name`, if the table has one.
    #[must_use]
    pub fn column(&self, name: ColumnName) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sexagesimal_text_reads_with_its_sign() {
        assert_eq!(
            sexagesimal("+14:35:30.0"),
            Some(14.0 + 35.0 / 60.0 + 0.5 / 60.0)
        );
        assert_eq!(sexagesimal("-00:30:00"), Some(-0.5));
        assert_eq!(
            sexagesimal("10 41 39.15").map(|h| h * 15.0),
            Some(160.413_125)
        );
        assert_eq!(sexagesimal("10:61:00"), None);
    }
}
