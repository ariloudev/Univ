//! The IAU Catalog of Star Names: a dated snapshot of the list of the IAU Working Group on Star
//! Names (WGSN), kept in the repository (`names/iau-wgsn.yaml`) because the WGSN publishes the
//! current list only as an HTML table. `scripts/snapshot-iau-names.py` writes it.
//!
//! Each name becomes a catalogue entry of the source whose identifier is the name itself, with
//! the star's HIP number, where it has one, as a cross-identifier: the `hip` cross-identification
//! joins it to the Hipparcos star by it, and the names rule attaches the name to that star. A
//! name without a HIP number keeps its sky position and, joining nothing, becomes an object of
//! its own with no distance. A name with neither is left out as missing its position.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use univ_model::entry::{CatalogueEntry, Flags, NativePosition, Position, PositionUncertainty};
use univ_model::ids::Designation;
use univ_model::position::Frame;
use univ_model::units::Unit;

use crate::normalize::{NormalizationReport, NormalizeError, Normalized};
use crate::source::{LeftOutReason, SourceDefinition};
use crate::yaml::read_checked;

/// The column a name's HIP number is kept under, as a cross-identifier.
pub const HIP: &str = "HIP";

/// The snapshot as its file holds it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// The page the list was read from.
    pub page: String,
    /// The date it was read, YYYY-MM-DD.
    pub read_on: String,
    /// Where the positions come from.
    pub positions: String,
    pub stars: Vec<StarName>,
}

/// One name of the list.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarName {
    pub name: String,
    /// The star's designation in another catalogue, such as `HR 2491`.
    #[serde(default)]
    pub designation: Option<String>,
    #[serde(default)]
    pub hip: Option<u32>,
    #[serde(default)]
    pub ra_deg: Option<f64>,
    #[serde(default)]
    pub dec_deg: Option<f64>,
    /// The date the WGSN adopted the name, YYYY-MM-DD.
    #[serde(default)]
    pub adopted: Option<String>,
}

impl Snapshot {
    /// The checks serde cannot make: a page, a date and a source of positions, and names that are
    /// neither blank nor given twice.
    fn check(&self) -> Result<(), (String, String)> {
        for (field, text) in [
            ("page", &self.page),
            ("read_on", &self.read_on),
            ("positions", &self.positions),
        ] {
            if text.trim().is_empty() {
                return Err((field.to_owned(), "is empty".to_owned()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for (i, star) in self.stars.iter().enumerate() {
            if star.name.trim().is_empty() {
                return Err((format!("stars[{i}].name"), "is empty".to_owned()));
            }
            if !seen.insert(star.name.as_str()) {
                return Err((format!("stars[{i}].name"), format!("repeats {}", star.name)));
            }
            if star.ra_deg.is_some() != star.dec_deg.is_some() {
                return Err((
                    format!("stars[{i}]"),
                    "gives one of ra_deg and dec_deg without the other".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

impl StarName {
    /// The entry of source `definition` the name gives, or why it is left out.
    fn entry(&self, definition: &SourceDefinition) -> Result<CatalogueEntry, LeftOutReason> {
        let designation = Designation::new(definition.id.clone(), self.name.clone())
            .map_err(|_| LeftOutReason::MissingIdentifier)?;
        let position = match (self.ra_deg, self.dec_deg) {
            (Some(lon), Some(lat)) => {
                let native = NativePosition {
                    frame: Frame::Icrs,
                    lon,
                    lat,
                    unit: Unit::Degree,
                    epoch: definition.epoch,
                };
                let position = Position::new(native, PositionUncertainty::NoneReported)
                    .map_err(|_| LeftOutReason::UnusableValue)?;
                Some(position)
            }
            _ if self.hip.is_none() => return Err(LeftOutReason::MissingPosition),
            _ => None,
        };
        let hip = self.hip.map(|hip| (Arc::from(HIP), hip.to_string()));
        let entry = CatalogueEntry {
            designation,
            name: self.designation.as_deref().map(Box::from),
            position,
            distances: thin_vec::thin_vec![],
            redshifts: thin_vec::thin_vec![],
            velocity: None,
            parallax: None,
            proper_motion: None,
            magnitude: None,
            colours: thin_vec::thin_vec![],
            spectral_type: None,
            group_id: None,
            supergalactic_velocity: None,
            flags: Flags::default(),
            cross_ids: Flags::named(hip),
            references: Box::default(),
        };
        entry.check().map_err(|_| LeftOutReason::UnusableValue)?;
        Ok(entry)
    }
}

/// Every name of the snapshot `file` as an entry of source `definition`, those left out counted
/// by reason.
///
/// # Errors
///
/// Fails when the file cannot be read or does not hold a snapshot.
pub fn normalize(definition: &SourceDefinition, file: &Path) -> Result<Normalized, NormalizeError> {
    let snapshot: Snapshot =
        read_checked(file, Snapshot::check).map_err(|e| NormalizeError::Table {
            source_id: definition.id.clone(),
            file: file.to_owned(),
            error: e.to_string(),
        })?;
    let mut report = NormalizationReport::default();
    let mut entries = Vec::with_capacity(snapshot.stars.len());
    for star in &snapshot.stars {
        match star.entry(definition) {
            Ok(entry) => {
                entries.push(entry);
                report.published += 1;
            }
            Err(reason) => *report.left_out.entry(reason).or_default() += 1,
        }
    }
    Ok(Normalized {
        source: definition.id.clone(),
        entries,
        report,
    })
}
