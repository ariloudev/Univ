//! One object of a release (look-up-an-object): every catalogue entry resolved to it, every
//! measurement as published beside its normalized value, the preferred measurement with the
//! ranking that chose it, and its derived position or why it has none.
//!
//! Every position names its frame and epoch. Values keep their unit, typed uncertainty and kind
//! of value; a value published without an uncertainty says so (`none_reported`), never zero.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cosmology::{AssumedCosmology, Cosmology};
use crate::derive::Step;
use crate::distance::{DistanceKind, Measure, Method};
use crate::entry::{PaperReference, PositionUncertainty, RedshiftKind};
use crate::ids::Designation;
use crate::object::{
    CandidateLink, Decided, FractionalUncertainty, MethodRank, NotPlacedReason, ObjectKind,
    PositionClass, Relation, Resolution,
};
use crate::position::Frame;
use crate::redshift::{DipoleModel, RedshiftFrame};
use crate::units::Unit;
use crate::value::{KindOfValue, Quantity};

/// One object of a release, with everything every source published for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectRecord {
    /// The release it was read from.
    pub release: u32,
    /// Its object id, stable across releases.
    pub id: u64,
    /// What its sources list it as: one body, or a galaxy group.
    pub kind: ObjectKind,
    /// Every catalogue entry resolved to it, numbered from 0, the anchor first.
    pub entries: Vec<EntryRecord>,
    /// Every designation it carries, each a source and the identifier that source gives: its
    /// entries' own and those their sources give beside them, such as a Hipparcos number.
    #[serde(default)]
    pub designations: Vec<Designation>,
    /// Every measurement of its entries, by entry, quantity and n.
    pub measurements: Vec<MeasurementRecord>,
    /// Its preferred sky position; absent when no entry has one.
    pub sky_position: Option<SkyPositionRecord>,
    /// The measurement that places it and the ranking that chose it; absent when it has no
    /// distance, parallax or redshift to rank.
    pub preferred: Option<PreferredMeasurement>,
    /// Its derived three-dimensional position; absent when it is not placed.
    pub position: Option<DerivedPositionRecord>,
    /// Why it is not placed; absent when it is.
    pub not_placed: Option<NotPlacedReason>,
    /// Each candidate match to another object, with its confidence: both stay separate
    /// objects, and no measurement is merged across the link.
    pub links: Vec<CandidateLink>,
    /// Typed relations to other objects, such as a group's dominant galaxy.
    pub relations: Vec<Relation>,
}

/// A catalogue entry of the object: the designation its source gives it, how it was resolved
/// into the object, and its sky position as published.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EntryRecord {
    /// Its number among the object's entries, from 0, as measurements name it.
    pub entry: u32,
    /// Its source's id, such as `gaia-dr3-bright`.
    pub source: String,
    /// Its identifier in that source.
    pub identifier: String,
    /// A second name its source gives, such as a Hipparcos number.
    pub name: Option<String>,
    /// How it was resolved into the object: the anchor, a published cross-identification, or a
    /// match with its method, separation, line-of-sight difference and confidence, each through
    /// the entry it names.
    pub resolved_by: Resolution,
    /// Its sky position as published; absent when its source publishes none.
    pub position: Option<EntryPosition>,
    /// The group its source assigns it to, by that source's group id.
    pub group_id: Option<String>,
    /// Its spectral type, character for character as its source publishes it, with its kind
    /// of value.
    pub spectral_type: Option<SpectralTypeRecord>,
    /// The papers a researched entry's values come from, the paper of its distance first;
    /// empty for an entry of a catalogue, which its source's citation covers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<PaperReference>,
    /// Its source's quality columns, by column name, as published.
    pub flags: BTreeMap<String, String>,
}

/// A spectral type as its source publishes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SpectralTypeRecord {
    pub text: String,
    /// Measured where the source classified the star's spectrum, estimated where it fitted a
    /// model to it.
    pub kind_of_value: KindOfValue,
}

/// An entry's sky position: as published, and in ICRS degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EntryPosition {
    /// The frame it was published in.
    pub frame: Frame,
    /// Longitude and latitude as published, in `unit`.
    pub lon: f64,
    pub lat: f64,
    pub unit: Unit,
    /// Its epoch as published, a Julian year.
    pub epoch: f64,
    /// Its ICRS right ascension and declination, degrees: at `epoch`, or at J2000 for a
    /// position published in FK4 B1950 or with its equinox unstated.
    pub ra: f64,
    pub dec: f64,
    /// Its errors as published, each beside its value in degrees, or none reported.
    pub uncertainty: PositionUncertainty,
}

/// A measured quantity of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MeasuredQuantity {
    Distance,
    Redshift,
    Parallax,
    /// A radial velocity, or a recession velocity cz, as published.
    Velocity,
    /// μα* = μα cos δ.
    ProperMotionRa,
    ProperMotionDec,
    /// An apparent magnitude in the band `band` names.
    Magnitude,
    /// A colour index between the two bands `band` names, such as `B-V`.
    ColourIndex,
    /// Supergalactic Cartesian velocity components (Lahav et al. 2000 axes).
    SupergalacticVelocityX,
    SupergalacticVelocityY,
    SupergalacticVelocityZ,
}

/// One measurement of an entry, its native value beside its normalized one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MeasurementRecord {
    /// The entry it belongs to, by its number.
    pub entry: u32,
    /// The source that published it: its entry's.
    pub source: String,
    pub quantity: MeasuredQuantity,
    /// Its place among the entry's values of this quantity, from 0.
    pub n: u32,
    /// Measured, estimated, inferred or reconstructed.
    pub kind_of_value: KindOfValue,
    /// The value as its source published it, with its unit and uncertainty.
    pub native: Quantity,
    /// Where the uncertainty comes from when the source states one for its whole catalogue,
    /// not one for each value: it is then estimated; absent when the source published it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty_basis: Option<UncertaintyBasis>,
    /// The value normalized; absent when it is the native value unchanged. For a redshift, the
    /// z of the frame it was published in.
    pub normalized: Option<Quantity>,
    /// What a distance is and how it was obtained.
    pub distance: Option<DistanceDetail>,
    /// What a redshift is, and its CMB-frame value.
    pub redshift: Option<RedshiftDetail>,
    /// A magnitude's band, as its source names the column, such as `Kcmag`; a colour index's
    /// two bands, such as `B-V`.
    pub band: Option<String>,
    /// The two magnitude columns a colour index was computed from, where its source publishes
    /// the magnitudes and not the colour.
    pub derived_from: Option<[String; 2]>,
}

/// An uncertainty that is not the source's for this value, and where it comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UncertaintyBasis {
    /// Estimated, for one stated for the whole catalogue.
    pub kind_of_value: KindOfValue,
    /// Where the source states it, quoted or cited.
    pub basis: String,
}

/// What a distance measurement is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DistanceDetail {
    pub kind: DistanceKind,
    /// The indicator or technique behind it.
    pub method: Method,
    /// Which distance it is: luminosity, comoving, Euclidean or unspecified.
    pub measure: Measure,
    /// The other form its source published beside the native value, kept and not used.
    pub also_published: Option<Quantity>,
    /// The cosmology the value was published for, where it depends on one.
    pub assumed_cosmology: Option<AssumedCosmology>,
}

/// What a redshift measurement is, with its CMB-frame value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RedshiftDetail {
    /// Spectroscopic or photometric.
    pub kind: RedshiftKind,
    /// The frame it was published in: heliocentric, or the CMB frame, where the dipole model
    /// only derives the heliocentric value back.
    pub published_frame: RedshiftFrame,
    /// The dipole model that relates its heliocentric and CMB-frame values.
    pub dipole: DipoleModel,
    /// Its CMB-frame redshift, dimensionless, with its uncertainty.
    pub cmb: Quantity,
}

/// A measurement of the object, by its entry, quantity and place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MeasurementKey {
    pub entry: u32,
    pub quantity: MeasuredQuantity,
    pub n: u32,
}

/// The measurement that places the object, and how the release's ranking rule chose it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PreferredMeasurement {
    /// The name of the release's ranking rule that chose it.
    pub rule: String,
    /// The distance, redshift or parallax preferred.
    pub measurement: MeasurementKey,
    /// Its method rank, which breaks a tie in fractional uncertainty.
    pub rank: MethodRank,
    /// The distance it gives, Mpc: as published for a distance, the inverse parallax, or the
    /// comoving distance of a redshift under the release cosmology.
    pub distance_mpc: f64,
    /// The fractional distance uncertainty it was ranked by, with its parts.
    pub uncertainty: FractionalUncertainty,
    /// What made it win.
    pub decided: Decided,
    /// The best of the measurements it was preferred over, where there was one.
    pub runner_up: Option<RunnerUp>,
}

/// The measurement a preferred one was preferred over.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RunnerUp {
    pub measurement: MeasurementKey,
    pub uncertainty: FractionalUncertainty,
}

/// The object's preferred sky position: ICRS degrees at its epoch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkyPositionRecord {
    /// Always `icrs`.
    pub frame: Frame,
    pub ra: f64,
    pub dec: f64,
    /// A Julian year.
    pub epoch: f64,
    /// The entry it is taken from, by its number.
    pub entry: u32,
    /// The class of position that chose it where the uncertainties could not.
    pub class: PositionClass,
    /// Its published error, the root mean square of σ(α cos δ) and σ(δ), milliarcseconds;
    /// absent when its source published none.
    pub uncertainty_mas: Option<f64>,
}

/// The object's derived position under the release cosmology.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DerivedPositionRecord {
    /// "ICRS equatorial Cartesian", as the tile manifests name it.
    pub frame: String,
    /// "Solar System barycentre".
    pub origin: String,
    /// The scale domain it is tiled in: `stars`, `nearby_galaxies` or `cosmic_web`.
    pub scale_domain: String,
    /// The scale domain's unit: parsecs for stars, megaparsecs otherwise.
    pub unit: Unit,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The epoch of its direction, its sky position's, a Julian year.
    pub epoch: f64,
    /// The frame its direction was published in, by its preferred position's entry.
    pub direction_frame: Frame,
    /// Its radial distance, Mpc, with its uncertainty.
    pub distance: Quantity,
    /// Which distance that is: comoving, or kept as published.
    pub measure: Measure,
    /// The value it was derived from, as published: a distance's or parallax's native value,
    /// or a redshift's CMB-frame value.
    pub native: Quantity,
    /// The cosmology the native value was published for, where it depends on one.
    pub assumed_cosmology: Option<AssumedCosmology>,
    /// The release cosmology it was placed under.
    pub cosmology: Cosmology,
    /// The transformations that produced it, in order, each with its citation.
    pub chain: Vec<Step>,
}
