//! The IAU Catalog of Star Names: a dated snapshot of the list of the IAU Working Group on Star
//! Names (WGSN), kept in the repository (`names/iau-wgsn.yaml`) because the WGSN gives the
//! current list as an HTML table on its site. `scripts/snapshot-iau-names.py` writes it.
//!
//! Each star becomes a catalogue entry of the source. A star with a HIP number is joined to the
//! Hipparcos star by it, through the `hip` cross-identification, and takes no position from the
//! table, whose positions are not attributed and are wrong for some stars; names the list gives
//! one HIP number, such as Izar and Pulcherrima, the two components of ε Boötis that Hipparcos
//! measured as one, are one entry carrying both, as an object holds one entry of a source. A
//! star with no HIP number keeps the table's position and, joining nothing, stands as an object
//! of its own with no distance; one with neither is left out as missing its position. A
//! definition's `replace_by_key` fixes, keyed by `name`, correct a HIP number the list has wrong.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use univ_model::entry::{CatalogueEntry, Flags, NativePosition, Position, PositionUncertainty};
use univ_model::ids::Designation;
use univ_model::position::Frame;
use univ_model::units::Unit;

use crate::normalize::{FixApplied, NormalizationReport, NormalizeError, Normalized};
use crate::source::{FixAction, LeftOutReason, SourceDefinition};
use crate::yaml::read_checked;

/// The column a name's HIP number is kept under, as a cross-identifier, as the source's
/// definition names it.
pub const HIP: &str = "hip";

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

/// The HIP number each fix of `definition` gives a star, by its name, with the change the fix
/// makes.
fn hip_fixes(definition: &SourceDefinition) -> Vec<(&str, u32, &str)> {
    let mut found = Vec::new();
    for fix in &definition.fixes {
        let Some(FixAction::ReplaceByKey {
            key,
            columns,
            values,
            ..
        }) = &fix.action
        else {
            continue;
        };
        let Some(at) = columns.iter().position(|c| c == "hip") else {
            continue;
        };
        if key != "name" {
            continue;
        }
        for (name, row) in values {
            // A HIP number written as a whole number in the fix, as every fix value is a number.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a HIP number is a positive whole number below 2^32, checked here"
            )]
            if let Some(&hip) = row.get(at)
                && hip.fract() == 0.0
                && (1.0..f64::from(u32::MAX)).contains(&hip)
            {
                found.push((name.as_str(), hip as u32, fix.change.as_str()));
            }
        }
    }
    found
}

/// The entry of source `definition` the names `stars` give, all of one star: the first name its
/// identifier, every name in its `name` field separated by commas, its HIP number a
/// cross-identifier, and the table's position only where it has no HIP number.
fn entry(
    definition: &SourceDefinition,
    stars: &[&StarName],
    hip: Option<u32>,
) -> Result<CatalogueEntry, LeftOutReason> {
    let first = stars.first().ok_or(LeftOutReason::MissingIdentifier)?;
    let designation = Designation::new(definition.id.clone(), first.name.clone())
        .map_err(|_| LeftOutReason::MissingIdentifier)?;
    let position = match (hip, first.ra_deg, first.dec_deg) {
        (Some(_), _, _) => None,
        (None, Some(lon), Some(lat)) => {
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
        (None, _, _) => return Err(LeftOutReason::MissingPosition),
    };
    let names: Vec<&str> = stars.iter().map(|s| s.name.as_str()).collect();
    let entry = CatalogueEntry {
        designation,
        name: Some(names.join(", ").into()),
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
        cross_ids: Flags::named(hip.map(|hip| (Arc::from(HIP), hip.to_string()))),
        references: Box::default(),
    };
    entry.check().map_err(|_| LeftOutReason::UnusableValue)?;
    Ok(entry)
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
    let fixes = hip_fixes(definition);
    let mut applied: BTreeMap<&str, u64> = BTreeMap::new();
    // Each star's HIP number, corrected by a fix where one names it.
    let hip_of = |star: &StarName| -> Option<u32> {
        match fixes.iter().find(|(name, _, _)| *name == star.name) {
            Some(&(_, hip, _)) => Some(hip),
            None => star.hip,
        }
    };
    // The stars by their HIP number, in the table's order; each star with none alone.
    let mut by_hip: BTreeMap<u32, Vec<&StarName>> = BTreeMap::new();
    let mut groups: Vec<(Option<u32>, Vec<&StarName>)> = Vec::new();
    for star in &snapshot.stars {
        if let Some((_, _, change)) = fixes.iter().find(|(name, _, _)| *name == star.name) {
            *applied.entry(change).or_default() += 1;
        }
        match hip_of(star) {
            Some(hip) => {
                let list = by_hip.entry(hip).or_default();
                if list.is_empty() {
                    groups.push((Some(hip), Vec::new()));
                }
                list.push(star);
            }
            None => groups.push((None, vec![star])),
        }
    }
    let mut report = NormalizationReport::default();
    let mut entries = Vec::with_capacity(groups.len());
    for (hip, mut stars) in groups {
        if let Some(hip) = hip {
            stars.clone_from(&by_hip[&hip]);
        }
        match entry(definition, &stars, hip) {
            Ok(entry) => {
                entries.push(entry);
                report.published += 1;
            }
            Err(reason) => *report.left_out.entry(reason).or_default() += 1,
        }
    }
    report.fixes = applied
        .into_iter()
        .map(|(change, entries_changed)| FixApplied {
            change: change.to_owned(),
            entries_changed,
        })
        .collect();
    Ok(Normalized {
        source: definition.id.clone(),
        entries,
        report,
    })
}
