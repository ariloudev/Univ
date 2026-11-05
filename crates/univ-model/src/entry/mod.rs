//! A catalogue entry: one row of a source, normalized, with each value kept as published beside
//! its normalized form.

mod colour;
mod position;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use colour::{ColourIndex, SpectralType};
pub use position::{NativePosition, Position, PositionErrors, PositionUncertainty};

use crate::cosmology::SPEED_OF_LIGHT;
use crate::distance::{Distance, DistanceError};
use crate::ids::Designation;
use crate::position::{Frame, PositionError, Vec3};
use crate::redshift::{CmbCorrection, CmbRedshift, RedshiftError, RedshiftFrame};
use crate::units::{Dimension, Unit};
use crate::value::{KindOfValue, Quantity, Value};
use position::in_unit;

/// How a redshift was measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RedshiftKind {
    Spectroscopic,
    Photometric,
}

/// A redshift as its source published it, heliocentric, with the CMB-frame redshift derived from
/// it and the dipole model of the derivation; or, where its source publishes only that, in the
/// CMB frame, kept as published.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawRedshift")]
pub struct Redshift {
    pub kind: RedshiftKind,
    pub kind_of_value: KindOfValue,
    /// The velocity cz as published, when the source gives cz rather than z: the redshift in
    /// its published frame is then cz / c, the definition of cz.
    pub velocity: Option<Quantity>,
    /// The heliocentric redshift as measured, and the CMB-frame redshift derived from it; or the
    /// CMB-frame redshift as published (see [`CmbRedshift::published`]).
    pub cmb: CmbRedshift,
}

/// A redshift as read, before its checks.
#[derive(Deserialize)]
struct RawRedshift {
    kind: RedshiftKind,
    kind_of_value: KindOfValue,
    velocity: Option<Quantity>,
    cmb: CmbRedshift,
}

impl TryFrom<RawRedshift> for Redshift {
    type Error = EntryError;
    fn try_from(raw: RawRedshift) -> Result<Self, EntryError> {
        let measured = raw.cmb.as_published();
        if measured.unit != Unit::Dimensionless || raw.cmb.value.unit != Unit::Dimensionless {
            return Err(EntryError::Unit("redshift", measured.unit));
        }
        if let Some(v) = raw.velocity {
            let z = v.to(Unit::KilometrePerSecond)?.value / SPEED_OF_LIGHT;
            if (z - measured.value).abs() > 1e-12 {
                return Err(EntryError::InconsistentRedshift);
            }
        }
        Ok(Self {
            kind: raw.kind,
            kind_of_value: raw.kind_of_value,
            velocity: raw.velocity,
            cmb: raw.cmb,
        })
    }
}

impl Redshift {
    /// A heliocentric redshift, published as z or as a velocity cz, moved to the CMB frame by
    /// `correction` toward the ICRS unit vector `direction`.
    ///
    /// # Errors
    ///
    /// Fails when `measured` is neither dimensionless nor a velocity, or the correction
    /// refuses it.
    pub fn heliocentric(
        measured: Quantity,
        kind: RedshiftKind,
        kind_of_value: KindOfValue,
        direction: Vec3,
        correction: &CmbCorrection,
    ) -> Result<Self, EntryError> {
        Self::measured_in(
            RedshiftFrame::Heliocentric,
            measured,
            kind,
            kind_of_value,
            direction,
            correction,
        )
    }

    /// A redshift relative to the Solar System barycentre, as DESI publishes it, moved to the
    /// CMB frame as a heliocentric one is and kept named barycentric.
    ///
    /// # Errors
    ///
    /// As [`Self::heliocentric`].
    pub fn barycentric(
        measured: Quantity,
        kind: RedshiftKind,
        kind_of_value: KindOfValue,
        direction: Vec3,
        correction: &CmbCorrection,
    ) -> Result<Self, EntryError> {
        Self::measured_in(
            RedshiftFrame::Barycentric,
            measured,
            kind,
            kind_of_value,
            direction,
            correction,
        )
    }

    fn measured_in(
        frame: RedshiftFrame,
        measured: Quantity,
        kind: RedshiftKind,
        kind_of_value: KindOfValue,
        direction: Vec3,
        correction: &CmbCorrection,
    ) -> Result<Self, EntryError> {
        let (z, velocity) = as_redshift(measured)?;
        let cmb = correction.apply(z, frame, direction)?;
        Ok(Self {
            kind,
            kind_of_value,
            velocity,
            cmb,
        })
    }

    /// A redshift its source published in the CMB frame, as z or as a velocity cz, kept as
    /// published: `correction` only derives its heliocentric redshift back, and the dipole step
    /// is not applied a second time.
    ///
    /// # Errors
    ///
    /// Fails when `published` is neither dimensionless nor a velocity, or `direction` is not a
    /// unit vector.
    pub fn published_cmb(
        published: Quantity,
        kind: RedshiftKind,
        kind_of_value: KindOfValue,
        direction: Vec3,
        correction: &CmbCorrection,
    ) -> Result<Self, EntryError> {
        let (z, velocity) = as_redshift(published)?;
        let cmb = correction.published_cmb(z, direction)?;
        Ok(Self {
            kind,
            kind_of_value,
            velocity,
            cmb,
        })
    }
}

/// `measured` as a dimensionless z, with the velocity cz it was published as, if it was one.
fn as_redshift(measured: Quantity) -> Result<(Quantity, Option<Quantity>), EntryError> {
    match measured.unit.dimension() {
        Dimension::Dimensionless => Ok((measured, None)),
        Dimension::Velocity => {
            let cz = measured.to(Unit::KilometrePerSecond)?;
            let z = Quantity::new(
                cz.value / SPEED_OF_LIGHT,
                Unit::Dimensionless,
                cz.uncertainty.scaled(1.0 / SPEED_OF_LIGHT),
            )?;
            Ok((z, Some(measured)))
        }
        _ => Err(EntryError::Unit("redshift", measured.unit)),
    }
}

/// A proper motion: μα* = μα cos δ and μδ, normalized to mas/yr.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProperMotion {
    pub ra: Value,
    pub dec: Value,
}

/// An apparent magnitude in the band its source names by its column, such as `Kcmag`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Magnitude {
    pub band: String,
    pub value: Value,
}

/// Supergalactic Cartesian coordinates in velocity space (Lahav et al. 2000 axes), normalized
/// to km/s: a direction times a velocity, not a position in length.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SupergalacticVelocity {
    pub x: Value,
    pub y: Value,
    pub z: Value,
}

/// One row of a source, normalized.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCatalogueEntry")]
pub struct CatalogueEntry {
    /// The source and the entry's identifier in it.
    pub designation: Designation,
    /// A second name the source gives, such as a Hipparcos number beside a Gaia `source_id`.
    pub name: Option<String>,
    /// The entry's position, or none when its source publishes no position: a source whose
    /// definition maps no position columns, such as the Bailer-Jones et al. 2021 distances or
    /// the Gaia DR3 to Hipparcos-2 cross-match, gives entries without one, and cross-matching
    /// attaches them by identifier (the Gaia `source_id`) to the entries of a source that has
    /// one. An entry is never published without a position for a blank cell: where a source
    /// maps position columns, a row whose position cell is blank is left out as missing its
    /// position.
    pub position: Option<Position>,
    pub distances: Vec<Distance>,
    pub redshifts: Vec<Redshift>,
    /// A radial velocity, or a recession velocity cz, as published.
    pub velocity: Option<Value>,
    /// A parallax, normalized to degrees as every angle is.
    pub parallax: Option<Value>,
    pub proper_motion: Option<Box<ProperMotion>>,
    pub magnitude: Option<Box<Magnitude>>,
    /// The colour indices its source publishes, or derives from two of its magnitudes.
    pub colours: Vec<ColourIndex>,
    pub spectral_type: Option<Box<SpectralType>>,
    pub group_id: Option<String>,
    pub supergalactic_velocity: Option<Box<SupergalacticVelocity>>,
    /// The source's quality columns, by column name, as published.
    pub flags: BTreeMap<String, String>,
    /// The papers a researched entry's values come from, the paper of its distance first; none
    /// for an entry of a catalogue, which its source's citation covers.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    pub references: Box<[PaperReference]>,
}

/// A paper a researched entry's values come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PaperReference {
    /// Authors, year, journal, volume and page, such as `Joyce et al. 2020, ApJ 902, 63`.
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibcode: Option<String>,
}
// The largest values an entry may have, which most entries lack, are boxed: an entry is held
// millions of times over.

/// An entry as read, before its checks.
#[derive(Deserialize)]
struct RawCatalogueEntry {
    designation: Designation,
    name: Option<String>,
    position: Option<Position>,
    distances: Vec<Distance>,
    redshifts: Vec<Redshift>,
    velocity: Option<Value>,
    parallax: Option<Value>,
    proper_motion: Option<Box<ProperMotion>>,
    magnitude: Option<Box<Magnitude>>,
    // Absent from entries normalized before colours were read.
    #[serde(default)]
    colours: Vec<ColourIndex>,
    #[serde(default)]
    spectral_type: Option<Box<SpectralType>>,
    group_id: Option<String>,
    supergalactic_velocity: Option<Box<SupergalacticVelocity>>,
    flags: BTreeMap<String, String>,
    // Absent from entries of catalogues.
    #[serde(default)]
    references: Box<[PaperReference]>,
}

impl TryFrom<RawCatalogueEntry> for CatalogueEntry {
    type Error = EntryError;
    fn try_from(raw: RawCatalogueEntry) -> Result<Self, EntryError> {
        let entry = Self {
            designation: raw.designation,
            name: raw.name,
            position: raw.position,
            distances: raw.distances,
            redshifts: raw.redshifts,
            velocity: raw.velocity,
            parallax: raw.parallax,
            proper_motion: raw.proper_motion,
            magnitude: raw.magnitude,
            colours: raw.colours,
            spectral_type: raw.spectral_type,
            group_id: raw.group_id,
            supergalactic_velocity: raw.supergalactic_velocity,
            flags: raw.flags,
            references: raw.references,
        };
        entry.check()?;
        Ok(entry)
    }
}

/// An entry, or a part of one, that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum EntryError {
    #[error("{0} is normalized to the wrong unit, {1:?}")]
    Unit(&'static str, Unit),
    #[error("a position is published in an equatorial frame, not {0:?}")]
    NotEquatorial(Frame),
    #[error("the ICRS position is not the one its native position gives")]
    InconsistentPosition,
    #[error("the heliocentric redshift is not the published velocity over c")]
    InconsistentRedshift,
    #[error("the distances of one entry are normalized to different units")]
    MixedDistanceUnits,
    #[error("{0} is blank")]
    Blank(&'static str),
    #[error(transparent)]
    Position(#[from] PositionError),
    #[error(transparent)]
    Distance(#[from] DistanceError),
    #[error(transparent)]
    Redshift(#[from] RedshiftError),
    #[error(transparent)]
    Value(#[from] crate::value::ValueError),
}

impl From<crate::units::UnitError> for EntryError {
    fn from(error: crate::units::UnitError) -> Self {
        Self::Value(error.into())
    }
}

impl CatalogueEntry {
    /// The redshift that stands for the entry where one is needed: its first spectroscopic
    /// redshift, else its first.
    #[must_use]
    pub fn preferred_redshift(&self) -> Option<&Redshift> {
        self.redshifts
            .iter()
            .find(|z| z.kind == RedshiftKind::Spectroscopic)
            .or_else(|| self.redshifts.first())
    }

    /// Checks that each value is normalized to its unit: degrees for angles, km/s for
    /// velocities, mas/yr for proper motions, and one of parsecs or megaparsecs for every
    /// distance of the entry.
    ///
    /// # Errors
    ///
    /// Fails, naming the field, on a value normalized to another unit or a blank name.
    pub fn check(&self) -> Result<(), EntryError> {
        if let Some(v) = &self.velocity {
            in_unit("velocity", v, Unit::KilometrePerSecond)?;
        }
        if let Some(p) = &self.parallax {
            in_unit("parallax", p, Unit::Degree)?;
        }
        if let Some(pm) = &self.proper_motion {
            for v in [&pm.ra, &pm.dec] {
                in_unit("proper motion", v, Unit::MilliarcsecondPerYear)?;
            }
        }
        if let Some(m) = &self.magnitude {
            in_unit("magnitude", &m.value, Unit::Magnitude)?;
        }
        for c in &self.colours {
            in_unit("colour index", &c.value, Unit::Magnitude)?;
        }
        if let Some(sg) = &self.supergalactic_velocity {
            for v in [&sg.x, &sg.y, &sg.z] {
                in_unit("supergalactic velocity", v, Unit::KilometrePerSecond)?;
            }
        }
        let mut units = self.distances.iter().map(|d| d.normalized.unit);
        if let Some(first) = units.next()
            && units.any(|u| u != first)
        {
            return Err(EntryError::MixedDistanceUnits);
        }
        let spectral_type = self.spectral_type.as_ref().map(|t| &t.text);
        for (field, text) in [
            ("name", self.name.as_ref()),
            ("group id", self.group_id.as_ref()),
            ("spectral type", spectral_type),
        ] {
            if text.is_some_and(|t| t.trim().is_empty()) {
                return Err(EntryError::Blank(field));
            }
        }
        if self.references.iter().any(|r| r.citation.trim().is_empty()) {
            return Err(EntryError::Blank("reference"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
