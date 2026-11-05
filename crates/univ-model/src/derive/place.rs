//! Cartesian positions: a direction in the ICRS and a radial distance, in ICRS, galactic and
//! supergalactic axes with the Solar System barycentre as origin.

use serde::Serialize;

use super::chain::{Step, Transformation};
use super::radial::Radial;
use super::{DeriveError, MODEL_VERSION, Release};
use crate::cosmology::{AssumedCosmology, Cosmology};
use crate::distance::{Distance, Measure};
use crate::entry::{Position, Redshift};
use crate::position::{Epoch, Frame, SkyPosition, Vec3, between, from_icrs, lon_lat, scaled};
use crate::value::{Quantity, Uncertainty};

/// A direction in the ICRS, with the frame it was published in and the systematic error, in
/// degrees, that frame carries: about 0.7° for an unstated equinox, none otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Direction {
    icrs: SkyPosition,
    published_in: Frame,
    systematic_degrees: Option<f64>,
}

impl Direction {
    /// The direction of longitude `lon` and latitude `lat`, in degrees, published in `frame`
    /// at `epoch`. A position in FK4 B1950 or of unstated equinox comes out at J2000, whose
    /// epoch it then has.
    ///
    /// # Errors
    ///
    /// Fails when the longitude or latitude lies outside its range.
    pub fn new(frame: Frame, lon: f64, lat: f64, epoch: Epoch) -> Result<Self, DeriveError> {
        let published = SkyPosition::new(lon, lat, epoch)?;
        let epoch = match frame {
            Frame::Fk4B1950 | Frame::EquatorialEquinoxUnstated => Epoch::J2000,
            _ => epoch,
        };
        let icrs = if frame == Frame::Icrs {
            published
        } else {
            let (ra, dec) = lon_lat(between(frame, Frame::Icrs).apply(published.unit_vector()));
            SkyPosition::new(ra, dec, epoch)?
        };
        Ok(Self {
            icrs,
            published_in: frame,
            systematic_degrees: frame.systematic_uncertainty(),
        })
    }

    /// The direction of a catalogue entry's position, already in the ICRS.
    #[must_use]
    pub fn of(position: &Position) -> Self {
        Self {
            icrs: position.icrs,
            published_in: position.native.frame,
            systematic_degrees: position.native.frame.systematic_uncertainty(),
        }
    }

    #[must_use]
    pub fn icrs(&self) -> SkyPosition {
        self.icrs
    }

    #[must_use]
    pub fn published_in(&self) -> Frame {
        self.published_in
    }

    #[must_use]
    pub fn systematic_degrees(&self) -> Option<f64> {
        self.systematic_degrees
    }
}

/// An object's position under the release cosmology: its direction, its radial distance as
/// derived and as published, and its Cartesian coordinates in Mpc in ICRS, galactic and
/// supergalactic axes, with the release cosmology, the chain that made it and the univ-model
/// version. It is made only from a [`Radial`], so never for an object without a distance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DerivedPosition {
    direction: Direction,
    native: Quantity,
    assumed: Option<AssumedCosmology>,
    distance: Quantity,
    measure: Measure,
    icrs: Vec3,
    galactic: Vec3,
    supergalactic: Vec3,
    cosmology: Cosmology,
    chain: Vec<Step>,
    model_version: &'static str,
}

impl DerivedPosition {
    /// The position `radial` along `direction`, recording each step after the radial chain.
    #[must_use]
    pub fn new(direction: Direction, radial: Radial) -> Self {
        let cosmology = radial.cosmology().clone();
        let frame = direction.published_in;
        // Allocated once at its length: a chain is held for each of millions of objects.
        let more = usize::from(direction.systematic_degrees.is_some())
            + usize::from(frame != Frame::Icrs)
            + 3;
        let mut chain = Vec::with_capacity(radial.chain().len() + more);
        chain.extend_from_slice(radial.chain());
        // An unstated equinox is first taken as J2000, then moved to the ICRS from there.
        if let Some(systematic_degrees) = direction.systematic_degrees {
            chain.push(Transformation::EquinoxAssumedJ2000 { systematic_degrees }.into());
        }
        if frame != Frame::Icrs {
            chain.push(Transformation::ToIcrs { from: frame }.into());
        }
        chain.push(Transformation::Cartesian.into());
        for to in [Frame::Galactic, Frame::Supergalactic] {
            chain.push(Transformation::Rotated { to }.into());
        }
        let (native, assumed, distance, measure) = radial.into_parts();
        let icrs = scaled(direction.icrs.unit_vector(), distance.value);
        Self {
            direction,
            native,
            assumed,
            distance,
            measure,
            icrs,
            galactic: from_icrs(Frame::Galactic).apply(icrs),
            supergalactic: from_icrs(Frame::Supergalactic).apply(icrs),
            cosmology,
            chain,
            model_version: MODEL_VERSION,
        }
    }

    #[must_use]
    pub fn direction(&self) -> &Direction {
        &self.direction
    }

    /// What the distance was derived from, as published.
    #[must_use]
    pub fn native(&self) -> &Quantity {
        &self.native
    }

    /// The cosmology the native value was published for, where it depends on one.
    #[must_use]
    pub fn assumed(&self) -> Option<&AssumedCosmology> {
        self.assumed.as_ref()
    }

    /// The radial distance in Mpc with its uncertainty.
    #[must_use]
    pub fn distance(&self) -> &Quantity {
        &self.distance
    }

    #[must_use]
    pub fn measure(&self) -> Measure {
        self.measure
    }

    /// The Cartesian position in Mpc in `frame`: ICRS, galactic or supergalactic.
    #[must_use]
    pub fn cartesian(&self, frame: Frame) -> Option<Vec3> {
        match frame {
            Frame::Icrs => Some(self.icrs),
            Frame::Galactic => Some(self.galactic),
            Frame::Supergalactic => Some(self.supergalactic),
            _ => None,
        }
    }

    /// The near and far ends, in `frame`, of the radial interval its distance's uncertainty
    /// gives; none for a distance without an interval, such as one with no error reported.
    #[must_use]
    pub fn radial_interval(&self, frame: Frame) -> Option<(Vec3, Vec3)> {
        let at = self.cartesian(frame)?;
        let d = self.distance.value;
        let (minus, plus) = match self.distance.uncertainty {
            Uncertainty::Symmetric { sigma } => (sigma, sigma),
            Uncertainty::Asymmetric { minus, plus }
            | Uncertainty::CredibleInterval { minus, plus, .. } => (minus, plus),
            _ => return None,
        };
        if d <= 0.0 {
            return None;
        }
        Some((
            scaled(at, (d - minus).max(0.0) / d),
            scaled(at, (d + plus) / d),
        ))
    }

    #[must_use]
    pub fn cosmology(&self) -> &Cosmology {
        &self.cosmology
    }

    #[must_use]
    pub fn chain(&self) -> &[Step] {
        &self.chain
    }

    #[must_use]
    pub fn model_version(&self) -> &'static str {
        self.model_version
    }
}

impl Release {
    /// Places an object from its preferred measurement: `distance` where it has one (rescaled
    /// at, and made comoving with, `redshift` when it is a luminosity distance), else the
    /// comoving distance of `redshift`. An object without a position, or with neither a distance nor a redshift,
    /// is not placed.
    ///
    /// # Errors
    ///
    /// Fails when the measurement cannot be derived, such as a blueshift with no distance.
    pub fn place(
        &self,
        position: Option<&Position>,
        distance: Option<&Distance>,
        redshift: Option<&Redshift>,
    ) -> Result<Option<DerivedPosition>, DeriveError> {
        let Some(position) = position else {
            return Ok(None);
        };
        let radial = match (distance, redshift) {
            (Some(d), z) => self.radial_from_distance(d, z)?,
            (None, Some(z)) => self.radial_from_redshift(&z.cmb)?,
            (None, None) => return Ok(None),
        };
        Ok(Some(DerivedPosition::new(Direction::of(position), radial)))
    }
}
