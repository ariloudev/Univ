//! A catalogue entry: one row of a source, normalized, with each value kept as published beside
//! its normalized form.

mod position;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
#[serde(rename_all = "snake_case")]
pub enum RedshiftKind {
    Spectroscopic,
    Photometric,
}

/// A redshift as its source published it, heliocentric, with the CMB-frame redshift derived from
/// it and the dipole model of the derivation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawRedshift")]
pub struct Redshift {
    pub kind: RedshiftKind,
    pub kind_of_value: KindOfValue,
    /// The velocity cz as published, when the source gives cz rather than z: the heliocentric
    /// redshift is then cz / c, the definition of cz.
    pub velocity: Option<Quantity>,
    /// The heliocentric redshift as measured, and the CMB-frame redshift derived from it.
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
        let measured = raw.cmb.heliocentric;
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
        let (z, velocity) = match measured.unit.dimension() {
            Dimension::Dimensionless => (measured, None),
            Dimension::Velocity => {
                let cz = measured.to(Unit::KilometrePerSecond)?;
                let z = Quantity::new(
                    cz.value / SPEED_OF_LIGHT,
                    Unit::Dimensionless,
                    cz.uncertainty.scaled(1.0 / SPEED_OF_LIGHT),
                )?;
                (z, Some(measured))
            }
            _ => return Err(EntryError::Unit("redshift", measured.unit)),
        };
        let cmb = correction.apply(z, RedshiftFrame::Heliocentric, direction)?;
        Ok(Self {
            kind,
            kind_of_value,
            velocity,
            cmb,
        })
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
    /// None for a source that publishes no position, such as a table of distances that names
    /// the entries of another source.
    pub position: Option<Position>,
    pub distances: Vec<Distance>,
    pub redshifts: Vec<Redshift>,
    /// A radial velocity, or a recession velocity cz, as published.
    pub velocity: Option<Value>,
    /// A parallax, normalized to degrees as every angle is.
    pub parallax: Option<Value>,
    pub proper_motion: Option<ProperMotion>,
    pub magnitude: Option<Magnitude>,
    pub group_id: Option<String>,
    pub supergalactic_velocity: Option<SupergalacticVelocity>,
    /// The source's quality columns, by column name, as published.
    pub flags: BTreeMap<String, String>,
}

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
    proper_motion: Option<ProperMotion>,
    magnitude: Option<Magnitude>,
    group_id: Option<String>,
    supergalactic_velocity: Option<SupergalacticVelocity>,
    flags: BTreeMap<String, String>,
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
            group_id: raw.group_id,
            supergalactic_velocity: raw.supergalactic_velocity,
            flags: raw.flags,
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
        for (field, text) in [("name", &self.name), ("group id", &self.group_id)] {
            if text.as_ref().is_some_and(|t| t.trim().is_empty()) {
                return Err(EntryError::Blank(field));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
