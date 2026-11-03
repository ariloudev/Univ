//! An entry's sky position: as its source published it, and in ICRS at the source's epoch.

use serde::{Deserialize, Serialize};

use super::EntryError;
use crate::position::{Epoch, Frame, SkyPosition, between, lon_lat, unit_vector};
use crate::units::{Dimension, Unit};
use crate::value::Value;

/// A position as its source published it: an equatorial longitude and latitude in a frame,
/// at an epoch, in a unit of angle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativePosition {
    pub frame: Frame,
    pub lon: f64,
    pub lat: f64,
    pub unit: Unit,
    pub epoch: Epoch,
}

/// The uncertainty of a position, each error as published beside its value in degrees.
/// Gaia and Hipparcos publish the error in right ascension as σ(α cos δ).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum PositionUncertainty {
    /// The source published no error for the position, which is not an error of zero.
    NoneReported,
    Published(Box<PositionErrors>),
}

/// The errors in right ascension and declination, as published and in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionErrors {
    pub ra: Value,
    pub dec: Value,
}

/// A position as published and in ICRS at the epoch of its source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPosition")]
pub struct Position {
    pub native: NativePosition,
    pub icrs: SkyPosition,
    pub uncertainty: PositionUncertainty,
}

/// A position as read, before its checks.
#[derive(Deserialize)]
struct RawPosition {
    native: NativePosition,
    icrs: SkyPosition,
    uncertainty: PositionUncertainty,
}

impl TryFrom<RawPosition> for Position {
    type Error = EntryError;
    fn try_from(raw: RawPosition) -> Result<Self, EntryError> {
        let position = Self::new(raw.native, raw.uncertainty)?;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let (ours, theirs) = (position.icrs, raw.icrs);
        // Right ascension wraps at 0 and 360.
        let ra = (ours.ra - theirs.ra + 180.0).rem_euclid(360.0) - 180.0;
        if !(near(ra, 0.0) && near(ours.dec, theirs.dec) && ours.epoch == theirs.epoch) {
            return Err(EntryError::InconsistentPosition);
        }
        Ok(position)
    }
}

impl Position {
    /// The position `native` in ICRS: an FK5 J2000 position rotated as Mignard & Froeschlé
    /// 2000 found (see [`Frame::Fk5J2000`]), an ICRS one kept, both at the native epoch.
    ///
    /// # Errors
    ///
    /// Fails when the native frame is not equatorial, its unit is not an angle, the position
    /// lies outside the sky, or an error is not an angle in degrees.
    pub fn new(
        native: NativePosition,
        uncertainty: PositionUncertainty,
    ) -> Result<Self, EntryError> {
        if !matches!(native.frame, Frame::Icrs | Frame::Fk5J2000) {
            return Err(EntryError::NotEquatorial(native.frame));
        }
        let factor = native
            .unit
            .factor_to(Unit::Degree)
            .map_err(|_| EntryError::Unit("position", native.unit))?;
        let (lon, lat) = (native.lon * factor, native.lat * factor);
        // Refused before the rotation, which would fold a latitude beyond a pole back in.
        SkyPosition::new(lon, lat, native.epoch)?;
        let (ra, dec) = if native.frame == Frame::Icrs {
            (lon, lat)
        } else {
            lon_lat(between(native.frame, Frame::Icrs).apply(unit_vector(lon, lat)))
        };
        if let PositionUncertainty::Published(errors) = &uncertainty {
            for error in [&errors.ra, &errors.dec] {
                in_unit("position error", error, Unit::Degree)?;
            }
        }
        Ok(Self {
            native,
            icrs: SkyPosition::new(ra, dec, native.epoch)?,
            uncertainty,
        })
    }
}

/// Refuses a value whose normalized unit is not `unit`, or whose native one measures another
/// dimension.
pub(super) fn in_unit(field: &'static str, value: &Value, unit: Unit) -> Result<(), EntryError> {
    let dimension: Dimension = unit.dimension();
    if value.normalized.unit != unit || value.native.unit.dimension() != dimension {
        return Err(EntryError::Unit(field, value.normalized.unit));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(frame: Frame, lon: f64, lat: f64) -> NativePosition {
        NativePosition {
            frame,
            lon,
            lat,
            unit: Unit::Degree,
            epoch: Epoch::J2000,
        }
    }

    #[test]
    fn an_fk5_position_is_kept_as_published_and_stored_in_icrs() {
        // The 6dFGS galaxy of position.rs, as astropy moves it.
        let p = Position::new(
            native(Frame::Fk5J2000, 54.430_541_7, -22.908_194_4),
            PositionUncertainty::NoneReported,
        )
        .unwrap();
        assert_eq!(p.native.lon, 54.430_541_7);
        assert!((p.icrs.ra - 54.430_537_566_576).abs() < 1e-11);
        assert!((p.icrs.dec + 22.908_191_374_027).abs() < 1e-11);
        assert_eq!(p.icrs.epoch, Epoch::J2000);
    }

    #[test]
    fn a_position_beyond_a_pole_or_in_a_non_equatorial_frame_is_refused() {
        let none = PositionUncertainty::NoneReported;
        assert!(Position::new(native(Frame::Fk5J2000, 10.0, 90.5), none.clone()).is_err());
        assert_eq!(
            Position::new(native(Frame::Galactic, 10.0, 10.0), none),
            Err(EntryError::NotEquatorial(Frame::Galactic))
        );
    }
}
