//! Derived positions: each placed object's distance along the line of sight under the release
//! cosmology, and its Cartesian position in ICRS, galactic and supergalactic coordinates with
//! the Solar System barycentre as origin. Every derived value records the release cosmology,
//! its chain of transformations and the univ-model version that computed it.
//!
//! The choices made where a published distance is not a comoving one:
//!
//! - A luminosity distance (one from a distance modulus) becomes comoving as `D_L / (1 + z)`
//!   with the CMB-frame redshift of the same entry. Without one it is kept as a luminosity
//!   distance, its measure stated and the reason in its chain: no redshift is invented. Peculiar
//!   velocities make the factor uncertain by about `v / c`, 0.1 % at 300 km/s, far below any
//!   distance indicator's error.
//! - A Euclidean distance (a parallax or a geometric distance) is taken as it is: within the
//!   Galaxy and the Local Group the cosmological distances differ from it by `H0 d / c`, under
//!   2e-4 at 1 Mpc, and its chain gives that fraction for its own distance.
//! - A distance whose source does not say which one it is is kept as published, and says so.
//! - An object with neither a distance nor a redshift has no [`Radial`] and so no position.

mod chain;
mod place;
mod radial;
mod table;

use thiserror::Error;

pub use chain::{Step, Transformation};
pub use place::{DerivedPosition, Direction};
pub use radial::{Radial, Rescaled, Rescaling};
pub use table::ComovingTable;

use crate::cosmology::{Cosmology, CosmologyError};
use crate::position::PositionError;
use crate::units::Unit;
use crate::value::ValueError;

/// The version of univ-model that derives a value, recorded in it.
pub const MODEL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The release cosmology with its comoving distance tabulated once, for every derivation of a
/// release.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    table: ComovingTable,
}

impl Release {
    /// A release under `cosmology`, as the release names it.
    #[must_use]
    pub fn new(cosmology: &Cosmology) -> Self {
        Self {
            table: ComovingTable::new(cosmology),
        }
    }

    /// A release under Planck 2018, the cosmology of every release that names no other.
    #[must_use]
    pub fn planck_2018() -> Self {
        Self::new(&Cosmology::planck_2018())
    }

    #[must_use]
    pub fn cosmology(&self) -> &Cosmology {
        self.table.cosmology()
    }

    #[must_use]
    pub fn table(&self) -> &ComovingTable {
        &self.table
    }
}

/// A value that cannot be derived.
#[derive(Debug, Error, PartialEq)]
pub enum DeriveError {
    #[error("the CMB-frame redshift {0} is a blueshift, which gives no distance")]
    Blueshift(f64),
    #[error("{0:?} is not a length to rescale")]
    NotALength(Unit),
    #[error("a length in h⁻¹ Mpc assumes H0 = 100 km/s/Mpc, not {0}")]
    HubbleScaled(f64),
    #[error(transparent)]
    Cosmology(#[from] CosmologyError),
    #[error(transparent)]
    Position(#[from] PositionError),
    #[error(transparent)]
    Value(#[from] ValueError),
}

impl From<crate::units::UnitError> for DeriveError {
    fn from(error: crate::units::UnitError) -> Self {
        Self::Value(error.into())
    }
}
