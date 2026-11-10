//! The extent rule: an entry that lies within a nearby galaxy's published extent at a velocity
//! agreeing with the galaxy's joins it as a piece, such as a 6dF fibre on an HII region of an
//! edge-on disc or a 2MASS detection of part of it. Point-source tolerances cannot join them.

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::units::Unit;

/// A source that publishes each galaxy's angular size, and the column it is read from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtentSource {
    pub source: SourceId,
    /// The column, kept among the entry's quality columns as published.
    pub column: String,
    pub form: ExtentForm,
    /// The unit of the size, once out of its logarithm.
    pub unit: Unit,
    /// Where the source states the column, its form and its unit.
    pub basis: String,
}

/// How a published size gives the radius of the circle the galaxy lies within.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtentForm {
    /// A major-axis diameter: the radius is half of it.
    Diameter,
    /// The log10 of a semi-major axis: the radius is ten to its power.
    Log10Radius,
}

/// The rule, as the release's matching rules state it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtentRule {
    /// Only a galaxy whose heliocentric cz lies below this takes pieces.
    pub below_km_s: f64,
    /// The most a piece's heliocentric cz may differ from the galaxy's.
    pub tolerance_km_s: f64,
    pub basis: String,
    pub sources: Vec<ExtentSource>,
    /// The sources whose entries may join as pieces: an object holding an entry of any other
    /// source is never one.
    pub pieces: Vec<SourceId>,
    pub pieces_basis: String,
    /// Sources that list each galaxy once under its own identity: a piece holding an entry of
    /// one of them never joins a galaxy that holds another entry of it.
    pub identities: Vec<SourceId>,
    pub identities_basis: String,
    /// Fibres of one survey on one nearby galaxy that no source gives an extent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fibres: Option<FibreRule>,
}

/// The fibre rule: below the extent rule's cz, fibres of one listed survey within `radius_arcsec`
/// and the extent rule's velocity tolerance of every fibre already joined are one galaxy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FibreRule {
    pub surveys: Vec<SourceId>,
    pub radius_arcsec: f64,
    pub basis: String,
}

impl ExtentRule {
    /// The checks serde cannot make: positive numbers, a basis for each choice, each source once.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem.
    pub fn check(&self) -> Result<(), (String, String)> {
        let err = |f: &str, p: &str| Err((format!("extents.{f}"), p.to_owned()));
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(self.below_km_s) {
            return err("below_km_s", "is not positive");
        }
        if !positive(self.tolerance_km_s) {
            return err("tolerance_km_s", "is not positive");
        }
        for (name, text) in [
            ("basis", &self.basis),
            ("pieces_basis", &self.pieces_basis),
            ("identities_basis", &self.identities_basis),
        ] {
            if text.trim().is_empty() {
                return err(name, "is blank");
            }
        }
        if let Some(f) = &self.fibres {
            if !positive(f.radius_arcsec) {
                return err("fibres.radius_arcsec", "is not positive");
            }
            if f.surveys.is_empty() || f.basis.trim().is_empty() {
                return err("fibres", "needs its surveys and its basis");
            }
        }
        for (i, s) in self.sources.iter().enumerate() {
            if s.column.trim().is_empty() || s.basis.trim().is_empty() {
                return err(&format!("sources[{i}]"), "needs its column and its basis");
            }
            if s.unit.factor_to(Unit::Degree).is_err() {
                return err(&format!("sources[{i}].unit"), "is not an angle");
            }
            if self.sources[..i].iter().any(|o| o.source == s.source) {
                return err(&format!("sources[{i}].source"), "is given twice");
            }
        }
        Ok(())
    }

    /// The radius, degrees, of the circle within which `entry` publishes its galaxy's extent:
    /// none when its source publishes none, or its cell is blank or not a positive number.
    #[must_use]
    pub fn radius_degrees(&self, entry: &CatalogueEntry) -> Option<f64> {
        let s = self
            .sources
            .iter()
            .find(|s| s.source == entry.designation.source)?;
        let published: f64 = entry.flags.get(&s.column)?.trim().parse().ok()?;
        let size = match s.form {
            ExtentForm::Diameter => published / 2.0,
            ExtentForm::Log10Radius => 10f64.powf(published),
        };
        let degrees = size * s.unit.factor_to(Unit::Degree).ok()?;
        (degrees.is_finite() && degrees > 0.0).then_some(degrees)
    }
}
