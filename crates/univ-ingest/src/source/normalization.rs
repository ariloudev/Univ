//! What a source definition says about reading its rows that the normalizer cannot infer: the
//! frame of each redshift, the kind and method of each distance and the Hubble constant it
//! assumes, rules on single rows, and the machine form of each fix.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::RedshiftKind;
use univ_model::redshift::RedshiftFrame;
use univ_model::units::{Dimension, Unit};
use univ_model::value::KindOfValue;

use super::{Column, Field, Fix};

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
}

/// How a source's rows are read beyond the column each field comes from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Normalization {
    #[serde(default)]
    pub redshifts: Vec<RedshiftReading>,
    #[serde(default)]
    pub distances: Vec<DistanceReading>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A redshift, published as z or as a velocity cz, with the frame its source states.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedshiftReading {
    pub column: String,
    #[serde(default)]
    pub error: Option<String>,
    pub frame: RedshiftFrame,
    #[serde(default)]
    pub kind: Option<RedshiftKind>,
    /// The kind of each row's redshift, by the code in a column.
    #[serde(default)]
    pub by_code: Option<RedshiftCodes>,
    /// Where the source states the frame and the kind, quoted or cited.
    pub basis: String,
}

/// The kind of a redshift by the code a column gives each row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedshiftCodes {
    pub column: String,
    pub codes: BTreeMap<String, RedshiftKind>,
}

/// A distance: the column it is read from, its error columns, its kind and method, either fixed
/// or by a code column, and the Hubble constant it was published for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistanceReading {
    /// A length, a length in h⁻¹ Mpc or a distance modulus.
    pub value: String,
    #[serde(default)]
    pub error: Option<ErrorColumns>,
    /// The other form the source publishes beside it, kept as published.
    #[serde(default)]
    pub also_published: Option<Published>,
    #[serde(default)]
    pub kind: Option<DistanceKind>,
    #[serde(default)]
    pub method: Option<Method>,
    /// The kind and method of each row, by the code in a column.
    #[serde(default)]
    pub by_code: Option<MethodCodes>,
    pub kind_of_value: KindOfValue,
    #[serde(default)]
    pub hubble_constant: Option<f64>,
    /// Where the source states the kind, the method and the constant.
    pub basis: String,
}

/// A value published beside a distance, with its error columns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Published {
    pub value: String,
    #[serde(default)]
    pub error: Option<ErrorColumns>,
}

/// The columns that give a value's uncertainty, and its form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ErrorColumns {
    /// One error, ±.
    Symmetric(String),
    /// The negative and positive errors.
    Asymmetric { minus: String, plus: String },
    /// The bounds of a credible interval of `probability`, such as the 16th and 84th
    /// percentiles of a posterior (0.68).
    CredibleInterval {
        low: String,
        high: String,
        probability: f64,
    },
}

/// The kind and method of a distance by the code a column gives each row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodCodes {
    pub column: String,
    pub codes: BTreeMap<String, Coded>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coded {
    pub kind: DistanceKind,
    pub method: Method,
}

/// A rule on the rows whose cell in a column is one of some values: they are left out, or some
/// of their values take another kind of value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub when: Condition,
    #[serde(default)]
    pub leave_out: Option<LeftOutReason>,
    #[serde(default)]
    pub kind_of_value: Option<KindOverride>,
    /// Where the source states what the code means.
    pub basis: String,
}

/// A row whose trimmed cell in `column` is one of `is`.
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

/// Fields a definition maps at most one column to. Quality columns are kept by name, and the
/// columns of distances and redshifts are named in `normalization`.
const SINGLE: [Field; 17] = [
    Field::Identifier,
    Field::Name,
    Field::RightAscension,
    Field::Declination,
    Field::RightAscensionError,
    Field::DeclinationError,
    Field::Velocity,
    Field::VelocityError,
    Field::Parallax,
    Field::ParallaxError,
    Field::ProperMotionRa,
    Field::ProperMotionDec,
    Field::Magnitude,
    Field::GroupId,
    Field::SupergalacticX,
    Field::SupergalacticY,
    Field::SupergalacticZ,
];

/// Fields whose columns a reading in `normalization` must name.
const READ: [Field; 10] = [
    Field::Distance,
    Field::DistanceLow,
    Field::DistanceHigh,
    Field::DistanceError,
    Field::DistanceModulus,
    Field::DistanceModulusError,
    Field::Redshift,
    Field::RedshiftError,
    Field::PhotometricRedshift,
    Field::PhotometricRedshiftError,
];

type Problem = (String, String);

fn problem(field: impl Into<String>, what: impl Into<String>) -> Problem {
    (field.into(), what.into())
}

/// The columns of a definition, by name.
struct Columns<'a>(&'a [Column]);

impl Columns<'_> {
    fn unit(&self, name: &str) -> Option<Unit> {
        self.0.iter().find(|c| c.column == name).map(|c| c.unit)
    }

    /// Refuses `name` unless it is a column of the definition.
    fn known(&self, at: &str, name: &str) -> Result<(), Problem> {
        self.unit(name)
            .map(|_| ())
            .ok_or_else(|| problem(at, format!("names {name}, which is not a column")))
    }
}

/// Checks the normalization block and the fixes' actions against the definition's columns.
pub(super) fn check(
    normalization: &Normalization,
    columns: &[Column],
    fixes: &[Fix],
) -> Result<(), Problem> {
    let table = Columns(columns);
    for field in &SINGLE {
        let mapped: Vec<&str> = columns
            .iter()
            .filter(|c| c.field == *field)
            .map(|c| c.column.as_str())
            .collect();
        if mapped.len() > 1 {
            return Err(problem(
                "columns",
                format!("map {} to {field:?}, which takes one", mapped.join(", ")),
            ));
        }
    }
    let mut named = HashSet::new();
    for (i, r) in normalization.redshifts.iter().enumerate() {
        named.extend(check_redshift(
            &table,
            &format!("normalization.redshifts[{i}]"),
            r,
        )?);
    }
    for (i, d) in normalization.distances.iter().enumerate() {
        named.extend(check_distance(
            &table,
            &format!("normalization.distances[{i}]"),
            d,
        )?);
    }
    if let Some(column) = columns
        .iter()
        .find(|c| READ.contains(&c.field) && !named.contains(c.column.as_str()))
    {
        return Err(problem(
            "normalization",
            format!(
                "names no reading of {}, whose {:?} needs one",
                column.column, column.field
            ),
        ));
    }
    for (i, rule) in normalization.rules.iter().enumerate() {
        check_rule(&table, &format!("normalization.rules[{i}]"), rule)?;
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

/// Checks a redshift reading, giving the columns it reads.
fn check_redshift<'a>(
    table: &Columns<'_>,
    at: &str,
    r: &'a RedshiftReading,
) -> Result<Vec<&'a str>, Problem> {
    blank(&format!("{at}.basis"), &r.basis)?;
    table.known(&format!("{at}.column"), &r.column)?;
    if !matches!(
        table.unit(&r.column).map(Unit::dimension),
        Some(Dimension::Dimensionless | Dimension::Velocity)
    ) {
        return Err(problem(at, "reads a column that is neither z nor cz"));
    }
    if r.frame != RedshiftFrame::Heliocentric {
        return Err(problem(
            format!("{at}.frame"),
            "only a heliocentric redshift is moved to the CMB frame",
        ));
    }
    if r.kind.is_some() == r.by_code.is_some() {
        return Err(problem(at, "gives either a kind or by_code"));
    }
    if let Some(codes) = &r.by_code {
        table.known(&format!("{at}.by_code.column"), &codes.column)?;
    }
    let mut read = vec![r.column.as_str()];
    if let Some(e) = &r.error {
        table.known(&format!("{at}.error"), e)?;
        read.push(e);
    }
    Ok(read)
}

/// Checks a distance reading, giving the columns it reads.
fn check_distance<'a>(
    table: &Columns<'_>,
    at: &str,
    d: &'a DistanceReading,
) -> Result<Vec<&'a str>, Problem> {
    blank(&format!("{at}.basis"), &d.basis)?;
    let one_method = d.kind.is_some() && d.method.is_some();
    let by_code = d.kind.is_none() && d.method.is_none() && d.by_code.is_some();
    if one_method == by_code {
        return Err(problem(at, "gives either a kind and a method, or by_code"));
    }
    if d.hubble_constant
        .is_some_and(|h| !(h.is_finite() && h > 0.0))
    {
        return Err(problem(format!("{at}.hubble_constant"), "is not positive"));
    }
    let errors: Vec<&ErrorColumns> = d
        .error
        .iter()
        .chain(d.also_published.iter().flat_map(|p| &p.error))
        .collect();
    for e in &errors {
        if let ErrorColumns::CredibleInterval { probability, .. } = e
            && !(*probability > 0.0 && *probability <= 1.0)
        {
            return Err(problem(at, "gives a probability outside (0, 1]"));
        }
    }
    let mut read = vec![d.value.as_str()];
    read.extend(errors.iter().flat_map(|e| e.columns()));
    read.extend(d.also_published.iter().map(|p| p.value.as_str()));
    read.extend(d.by_code.iter().map(|c| c.column.as_str()));
    for name in &read {
        table.known(at, name)?;
    }
    Ok(read)
}

fn check_rule(table: &Columns<'_>, at: &str, rule: &Rule) -> Result<(), Problem> {
    blank(&format!("{at}.basis"), &rule.basis)?;
    table.known(&format!("{at}.when.column"), &rule.when.column)?;
    if rule.when.is.is_empty() {
        return Err(problem(format!("{at}.when.is"), "lists no value"));
    }
    match (&rule.leave_out, &rule.kind_of_value) {
        (Some(_), None) => Ok(()),
        (None, Some(k)) => k
            .columns
            .iter()
            .try_for_each(|c| table.known(&format!("{at}.kind_of_value.columns"), c)),
        _ => Err(problem(at, "either leaves out or sets a kind of value")),
    }
}

fn blank(field: &str, value: &str) -> Result<(), Problem> {
    if value.trim().is_empty() {
        Err(problem(field, "is empty"))
    } else {
        Ok(())
    }
}

impl ErrorColumns {
    /// The columns the error is read from.
    #[must_use]
    pub fn columns(&self) -> Vec<&str> {
        match self {
            Self::Symmetric(c) => vec![c],
            Self::Asymmetric { minus, plus } => vec![minus, plus],
            Self::CredibleInterval { low, high, .. } => vec![low, high],
        }
    }
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
