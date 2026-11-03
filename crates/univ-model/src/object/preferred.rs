//! An object's preferred measurements: the distance that places it, the sky position it takes,
//! and whether it can be placed in three dimensions at all.

use serde::{Deserialize, Serialize};

use crate::distance::DistanceKind;
use crate::entry::RedshiftKind;
use crate::ids::Designation;

/// The method ranks that break a tie in fractional distance uncertainty, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodRank {
    /// A parallax, or a distance derived from one alone or with a photometric prior.
    Geometric,
    PrimaryIndicator,
    SecondaryIndicator,
    SpectroscopicRedshift,
    PhotometricRedshift,
}

impl From<DistanceKind> for MethodRank {
    fn from(kind: DistanceKind) -> Self {
        match kind {
            DistanceKind::Parallax | DistanceKind::Geometric | DistanceKind::Photogeometric => {
                Self::Geometric
            }
            DistanceKind::PrimaryIndicator => Self::PrimaryIndicator,
            DistanceKind::SecondaryIndicator => Self::SecondaryIndicator,
            DistanceKind::SpectroscopicRedshift => Self::SpectroscopicRedshift,
            DistanceKind::PhotometricRedshift => Self::PhotometricRedshift,
        }
    }
}

impl From<RedshiftKind> for MethodRank {
    fn from(kind: RedshiftKind) -> Self {
        match kind {
            RedshiftKind::Spectroscopic => Self::SpectroscopicRedshift,
            RedshiftKind::Photometric => Self::PhotometricRedshift,
        }
    }
}

/// A measurement of one of an object's entries, by its place in the entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "measurement")]
pub enum MeasurementRef {
    /// The entry's distance at `index` in its list of distances.
    Distance { entry: Designation, index: usize },
    /// The entry's redshift at `index`, whose CMB-frame value gives a comoving distance under
    /// the release cosmology.
    Redshift { entry: Designation, index: usize },
    /// The entry's parallax, inverted.
    Parallax { entry: Designation },
}

impl MeasurementRef {
    #[must_use]
    pub fn entry(&self) -> &Designation {
        match self {
            Self::Distance { entry, .. }
            | Self::Redshift { entry, .. }
            | Self::Parallax { entry } => entry,
        }
    }
}

/// The fractional distance uncertainty, the error over the distance, that a measurement was ranked by.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum FractionalUncertainty {
    /// `total` is what was compared: the measurement's own term and, for a redshift distance,
    /// the peculiar-velocity term, added in quadrature. A redshift published without an error
    /// has no measurement term.
    Reported {
        total: f64,
        measurement: Option<f64>,
        peculiar_velocity: Option<f64>,
    },
    /// The source published no uncertainty the rule can compare, such as none at all or an
    /// upper limit: the measurement ranks after every one with an uncertainty.
    NoneReported,
}

impl FractionalUncertainty {
    /// The fraction compared, or none.
    #[must_use]
    pub fn total(&self) -> Option<f64> {
        match self {
            Self::Reported { total, .. } => Some(*total),
            Self::NoneReported => None,
        }
    }
}

/// What made the preferred measurement win.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decided {
    /// It was the object's only candidate.
    OnlyCandidate,
    /// Its fractional uncertainty was the smallest, beyond the tie tolerance.
    Uncertainty,
    /// Its uncertainty tied with another's, and its method ranks higher.
    MethodRank,
    /// No candidate reported an uncertainty, and its method ranks highest.
    MethodRankNoneReported,
}

/// The measurement that places an object, and the uncertainty that made it win.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreferredDistance {
    pub measurement: MeasurementRef,
    pub rank: MethodRank,
    /// The distance it gives, Mpc: the published distance for a distance, the inverse
    /// parallax for a parallax, the comoving distance under the release cosmology for a
    /// redshift. Converting each to a comoving distance is the positions stage's work.
    pub distance_mpc: f64,
    pub uncertainty: FractionalUncertainty,
    pub decided: Decided,
    /// The best of the measurements it beat, with its uncertainty, where there was one.
    pub runner_up: Option<(MeasurementRef, FractionalUncertainty)>,
}

/// The classes of sky position, best first, where the published uncertainties cannot decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionClass {
    Gaia,
    Hipparcos,
    Survey,
}

/// The entry whose sky position the object takes, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreferredPosition {
    pub entry: Designation,
    pub class: PositionClass,
    /// The published position error, the root mean square of σ(α cos δ) and σ(δ), in
    /// milliarcseconds; none when the source published none.
    pub uncertainty_mas: Option<f64>,
}

/// Why an object has no three-dimensional position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotPlacedReason {
    /// None of its entries carries a distance, a usable parallax or a positive redshift.
    NoDistance,
}

/// Whether an object can be placed in three dimensions. An object not placed can be looked up
/// with its sky position alone, and appears in no tile and no bounded volume query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "placed")]
pub enum Placement {
    /// It has a preferred distance.
    #[serde(rename = "yes")]
    Placed,
    #[serde(rename = "no")]
    NotPlaced { reason: NotPlacedReason },
}

impl Placement {
    #[must_use]
    pub fn is_placed(self) -> bool {
        self == Self::Placed
    }
}
