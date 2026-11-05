//! What a source definition says about reading its rows that the normalizer cannot infer: the
//! frame of each redshift, the kind and method of each distance and the Hubble constant it
//! assumes. Rules on single rows and the machine form of each fix are in `rules`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use univ_model::cosmology::{Cosmology, CosmologyError};
use univ_model::distance::{DistanceKind, Measure, Method};
use univ_model::entry::RedshiftKind;
use univ_model::redshift::RedshiftFrame;
use univ_model::units::{Dimension, Unit};
use univ_model::value::KindOfValue;

pub use catalogue_error::CatalogueError;

use super::rules::{self, Rule};
use super::{Column, Field, Fix};

mod catalogue_error;

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
    #[serde(default)]
    pub colours: Vec<ColourReading>,
    #[serde(default)]
    pub spectral_type: Option<SpectralTypeReading>,
}

/// A colour index: published in `column`, or derived from the two magnitude columns `from`
/// where the source publishes magnitudes and not the colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColourReading {
    /// The two bands, the one subtracted from second, such as `[B, V]`.
    pub bands: [String; 2],
    #[serde(default)]
    pub column: Option<String>,
    /// The column of its symmetric error, where the source publishes one.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub from: Option<[String; 2]>,
}

/// A spectral type published in `column`, of the kind of value `basis` justifies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralTypeReading {
    pub column: String,
    pub kind_of_value: KindOfValue,
    /// What the source writes in the column where it gives no type, such as Gaia ESP-HS's
    /// `unknown`: read as no spectral type, as a blank cell is.
    #[serde(default)]
    pub none: Vec<String>,
    /// Where the source states how it classified its stars, quoted or cited.
    pub basis: String,
}

/// A redshift, published as z or as a velocity cz, with the frame its source states.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedshiftReading {
    pub column: String,
    #[serde(default)]
    pub error: Option<String>,
    /// One uncertainty the source states for its whole catalogue rather than for each row, in
    /// place of `error`.
    #[serde(default)]
    pub catalogue_error: Option<CatalogueError>,
    pub frame: RedshiftFrame,
    #[serde(default)]
    pub kind: Option<RedshiftKind>,
    /// The kind of each row's redshift, by the code in a column.
    #[serde(default)]
    pub by_code: Option<RedshiftCodes>,
    /// Where given, a velocity from `from_km_s` up stands for a comoving distance, not for cz.
    #[serde(default)]
    pub comoving_velocity: Option<ComovingVelocity>,
    /// Where the source states the frame and the kind, quoted or cited.
    pub basis: String,
}

/// A velocity column whose values from `from_km_s` up are H0 times a comoving distance in a
/// flat ΛCDM cosmology, `hubble_constant` and `omega_matter`: read as the redshift at that
/// distance, not as cz.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComovingVelocity {
    pub from_km_s: f64,
    pub hubble_constant: f64,
    pub omega_matter: f64,
    /// The kind of value of a redshift so read: inferred where the source does not state the
    /// reading and the definition infers it from evidence.
    pub kind_of_value: KindOfValue,
    /// Why the values are so read, with the evidence.
    pub basis: String,
}

impl ComovingVelocity {
    /// The velocity it starts from, km/s, its cosmology and the kind of value of what it reads.
    ///
    /// # Errors
    ///
    /// Fails on a Hubble constant or matter density the cosmology refuses.
    pub fn read(&self) -> Result<(f64, Cosmology, KindOfValue), CosmologyError> {
        Cosmology::new("comoving velocity", self.hubble_constant, self.omega_matter)
            .map(|c| (self.from_km_s, c, self.kind_of_value))
    }
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
    /// Which distance it is, as the source states it: a luminosity distance for a modulus.
    pub measure: Measure,
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
const READ: [Field; 14] = [
    Field::ColourIndex,
    Field::ColourIndexError,
    Field::ColourMagnitude,
    Field::SpectralType,
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

pub(super) type Problem = (String, String);

pub(super) fn problem(field: impl Into<String>, what: impl Into<String>) -> Problem {
    (field.into(), what.into())
}

/// The columns of a definition, by name.
pub(super) struct Columns<'a>(&'a [Column]);

impl Columns<'_> {
    fn unit(&self, name: &str) -> Option<Unit> {
        self.0.iter().find(|c| c.column == name).map(|c| c.unit)
    }

    /// Refuses `name` unless it is a column of the definition.
    pub(super) fn known(&self, at: &str, name: &str) -> Result<(), Problem> {
        self.unit(name)
            .map(|_| ())
            .ok_or_else(|| problem(at, format!("names {name}, which is not a column")))
    }
}

/// The columns a colour reading names, each a column of the definition in magnitudes.
/// The photometric bands a colour index may lie between: Johnson–Cousins `U`, `B`, `V`, `R`
/// and `I`, Gaia's `G`, `BP` and `RP`, and 2MASS's `J`, `H` and `Ks`, as the catalogues name
/// them. A band outside them is refused, so a misspelling cannot leave a colour unread.
const BANDS: [&str; 11] = ["U", "B", "V", "R", "I", "G", "BP", "RP", "J", "H", "Ks"];

fn check_colour<'c>(
    table: &Columns<'_>,
    at: &str,
    c: &'c ColourReading,
) -> Result<Vec<&'c str>, Problem> {
    if let Some(band) = c.bands.iter().find(|b| !BANDS.contains(&b.as_str())) {
        return Err(problem(
            at,
            format!(
                "names the band {band}, which is not one of {}",
                BANDS.join(", ")
            ),
        ));
    }
    if c.bands[0] == c.bands[1] {
        return Err(problem(at, format!("names the band {} twice", c.bands[0])));
    }
    let names: Vec<&str> = c
        .column
        .iter()
        .chain(&c.error)
        .chain(c.from.iter().flatten())
        .map(String::as_str)
        .collect();
    for name in &names {
        table.known(at, name)?;
        if table.unit(name) != Some(Unit::Magnitude) {
            return Err(problem(
                at,
                format!("names {name}, which is not in magnitudes"),
            ));
        }
    }
    Ok(names)
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
            normalization.redshifts.len() == 1,
        )?);
    }
    for (i, d) in normalization.distances.iter().enumerate() {
        named.extend(check_distance(
            &table,
            &format!("normalization.distances[{i}]"),
            d,
        )?);
    }
    for (i, c) in normalization.colours.iter().enumerate() {
        named.extend(check_colour(
            &table,
            &format!("normalization.colours[{i}]"),
            c,
        )?);
    }
    if let Some(s) = &normalization.spectral_type {
        table.known("normalization.spectral_type", &s.column)?;
        named.insert(s.column.as_str());
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
        rules::check_rule(
            &table,
            &normalization.distances,
            &format!("normalization.rules[{i}]"),
            rule,
        )?;
    }
    rules::check_fixes(&table, fixes)?;
    Ok(())
}

/// Checks a redshift reading, giving the columns it reads.
fn check_redshift<'a>(
    table: &Columns<'_>,
    at: &str,
    r: &'a RedshiftReading,
    alone: bool,
) -> Result<Vec<&'a str>, Problem> {
    blank(&format!("{at}.basis"), &r.basis)?;
    table.known(&format!("{at}.column"), &r.column)?;
    if !matches!(
        table.unit(&r.column).map(Unit::dimension),
        Some(Dimension::Dimensionless | Dimension::Velocity)
    ) {
        return Err(problem(at, "reads a column that is neither z nor cz"));
    }
    if r.frame == RedshiftFrame::LocalGroup {
        return Err(problem(
            format!("{at}.frame"),
            "only a heliocentric or barycentric redshift, moved to the CMB frame, or one \
             published in the CMB frame is read",
        ));
    }
    if r.kind.is_some() == r.by_code.is_some() {
        return Err(problem(at, "gives either a kind or by_code"));
    }
    if let Some(codes) = &r.by_code {
        table.known(&format!("{at}.by_code.column"), &codes.column)?;
    }
    if let Some(c) = &r.comoving_velocity {
        let at = format!("{at}.comoving_velocity");
        blank(&format!("{at}.basis"), &c.basis)?;
        if table.unit(&r.column).map(Unit::dimension) != Some(Dimension::Velocity) {
            return Err(problem(&at, "reads a column that is not a velocity"));
        }
        if !(c.from_km_s.is_finite() && c.from_km_s >= 0.0) {
            return Err(problem(&at, "starts at a velocity that is negative"));
        }
        c.read().map_err(|e| problem(&at, e.to_string()))?;
    }
    if let Some(c) = &r.catalogue_error {
        if !alone {
            return Err(problem(
                format!("{at}.catalogue_error"),
                "is given on one of several redshifts: each value of the source would be taken \
                 as having it",
            ));
        }
        catalogue_error::check(
            &format!("{at}.catalogue_error"),
            c,
            table.unit(&r.column),
            r.error.is_some(),
            r.by_code.is_some(),
        )?;
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
    if table.unit(&d.value) == Some(Unit::Magnitude) && d.measure != Measure::Luminosity {
        return Err(problem(
            format!("{at}.measure"),
            "a distance from a distance modulus is a luminosity distance",
        ));
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

pub(super) fn blank(field: &str, value: &str) -> Result<(), Problem> {
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
