//! Rules on single rows of a source and the machine form of each fix: what the normalizer does
//! to the rows that meet a condition, and why a row is left out.

use serde::{Deserialize, Serialize};
use univ_model::distance::{DistanceKind, Method};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::Fix;
use super::normalization::{Columns, DistanceReading, Problem, blank, problem};

/// Why a catalogue entry is left out of a release. Every entry left out is counted under one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftOutReason {
    /// The identifier cell is blank.
    MissingIdentifier,
    /// A position cell is blank in a source that publishes positions.
    MissingPosition,
    /// A cell that holds a number does not parse as one.
    UnparsableNumber,
    RightAscensionOutOfRange,
    DeclinationOutOfRange,
    /// An error is negative or not a finite number, or a credible bound lies on the wrong side
    /// of its value.
    BadUncertainty,
    /// A value parses but cannot stand, such as a distance modulus too large for a distance.
    UnusableValue,
    /// A distance's method code is not one the definition maps.
    UnknownMethodCode,
    /// The source's distance is an estimate from a velocity or a texture, not a measurement.
    DistanceNotMeasured,
    /// The entry is simulated, such as a 2M++ galaxy filling the Zone of Avoidance.
    SimulatedObject,
    /// The source marks the redshift unsuitable, as 6dFGS does with a zero error.
    UnsuitableRedshift,
    /// Every cell of the row is blank: an empty line inside a table, or a line of empty cells.
    BlankRow,
}

/// A rule on the rows whose cells meet a condition, and every condition in `and`: they are left
/// out, some of their values take another kind of value, or a distance takes another kind and
/// method. A rule does one of these.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub when: Condition,
    #[serde(default)]
    pub and: Vec<Condition>,
    #[serde(default)]
    pub leave_out: Option<LeftOutReason>,
    #[serde(default)]
    pub kind_of_value: Option<KindOverride>,
    #[serde(default)]
    pub distance: Option<DistanceOverride>,
    /// Where the source states what the cells mean.
    pub basis: String,
}

/// The distance read from the column `value` takes this kind and method, such as a group
/// distance that is a direct calibration where the source marks no other measurement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistanceOverride {
    pub value: String,
    pub kind: DistanceKind,
    pub method: Method,
}

/// A row whose trimmed cell in `column` is one of `is`: the same text, or the same number, so
/// that `0` matches a cell `0.000`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub column: String,
    pub is: Vec<String>,
}

/// The values of `columns` take the kind of value `kind`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindOverride {
    pub columns: Vec<String>,
    pub kind: KindOfValue,
}

/// What a fix changes, applied by the normalizer. A fix without one is expressed by the
/// definition itself, such as a column's unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FixAction {
    /// The columns hold values in `unit`, whatever their published unit says.
    ReadAs { columns: Vec<String>, unit: Unit },
    /// An error of zero in these columns means the error was not reported.
    ZeroErrorIsNoneReported { columns: Vec<String> },
    /// An error of zero in these columns marks the row, which is left out for `reason`.
    ZeroErrorLeavesOut {
        columns: Vec<String>,
        reason: LeftOutReason,
    },
    /// On the rows that meet `when`, the values of `columns` are dropped and the entry kept, as
    /// if the cells were blank.
    DropWhen {
        columns: Vec<String>,
        when: Condition,
    },
}

/// The most fixes a definition may have: the normalizer counts the entries each changed with a
/// bit per fix.
pub const MAX_FIXES: usize = 64;

impl Condition {
    /// Whether `cell`, trimmed, is one of the condition's values.
    #[must_use]
    #[expect(
        clippy::float_cmp,
        reason = "the same number as written, not a near one"
    )]
    pub fn matches(&self, cell: &str) -> bool {
        let number = cell.parse::<f64>().ok();
        self.is
            .iter()
            .any(|v| v == cell || number.is_some_and(|n| v.parse::<f64>().is_ok_and(|x| x == n)))
    }
}

pub(super) fn check_rule(
    table: &Columns<'_>,
    distances: &[DistanceReading],
    at: &str,
    rule: &Rule,
) -> Result<(), Problem> {
    blank(&format!("{at}.basis"), &rule.basis)?;
    for (n, c) in std::iter::once(&rule.when).chain(&rule.and).enumerate() {
        let at = if n == 0 {
            format!("{at}.when")
        } else {
            format!("{at}.and[{}]", n - 1)
        };
        table.known(&format!("{at}.column"), &c.column)?;
        if c.is.is_empty() {
            return Err(problem(format!("{at}.is"), "lists no value"));
        }
    }
    match (&rule.leave_out, &rule.kind_of_value, &rule.distance) {
        (Some(_), None, None) => Ok(()),
        (None, Some(k), None) => k
            .columns
            .iter()
            .try_for_each(|c| table.known(&format!("{at}.kind_of_value.columns"), c)),
        (None, None, Some(d)) => {
            if !distances.iter().any(|r| r.value == d.value) {
                return Err(problem(
                    format!("{at}.distance.value"),
                    format!("{} is not the value of a distance reading", d.value),
                ));
            }
            if matches!(&d.method, Method::Mixed(m) if m.len() < 2) {
                return Err(problem(
                    format!("{at}.distance.method"),
                    "a mixed method lists at least two methods",
                ));
            }
            Ok(())
        }
        _ => Err(problem(
            at,
            "does one of: leaves out, sets a kind of value, or sets a distance's kind and method",
        )),
    }
}

pub(super) fn check_fixes(table: &Columns<'_>, fixes: &[Fix]) -> Result<(), Problem> {
    if fixes.len() > MAX_FIXES {
        return Err(problem(
            "fixes",
            format!("lists {} fixes, more than {MAX_FIXES}", fixes.len()),
        ));
    }
    for (i, fix) in fixes.iter().enumerate() {
        let Some(action) = &fix.action else { continue };
        let at = format!("fixes[{i}].action.columns");
        if action.columns().is_empty() {
            return Err(problem(at, "lists no column"));
        }
        for c in action.columns() {
            table.known(&at, c)?;
        }
        if let FixAction::DropWhen { when, .. } = action {
            let at = format!("fixes[{i}].action.when");
            table.known(&format!("{at}.column"), &when.column)?;
            if when.is.is_empty() {
                return Err(problem(at, "lists no value"));
            }
        }
    }
    Ok(())
}

impl FixAction {
    /// The columns the fix changes.
    #[must_use]
    pub fn columns(&self) -> &[String] {
        match self {
            Self::ReadAs { columns, .. }
            | Self::ZeroErrorIsNoneReported { columns }
            | Self::ZeroErrorLeavesOut { columns, .. }
            | Self::DropWhen { columns, .. } => columns,
        }
    }
}
