//! A structure's position and geometry as its papers publish them: one of univ-model's geometry
//! kinds, in a named frame, with every coordinate a cited value in its published unit. Turning
//! them into a univ-model shape, which needs a cosmology for redshifts and velocities, is left to
//! the pipeline.

use serde::{Deserialize, Serialize};
use univ_model::position::Frame;
use univ_model::units::{Dimension, Unit};

use super::cited::Cited;

/// The frame a structure's coordinates are published in. Equatorial frames carry their equinox;
/// the epoch of extragalactic positions does not matter, having no proper motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureFrame {
    Icrs,
    Galactic,
    Supergalactic,
    #[serde(rename = "fk5_j2000")]
    Fk5J2000,
    /// FK4 at equinox B1950, which univ-model does not yet rotate: the pipeline precesses it.
    #[serde(rename = "fk4_b1950")]
    Fk4B1950,
    /// Equatorial coordinates whose paper does not state the equinox. Kept as published: B1950
    /// and J2000 differ by up to about a degree here, which the pipeline must carry as part of
    /// the position's uncertainty.
    EquatorialEquinoxUnstated,
}

impl StructureFrame {
    /// The univ-model frame, where univ-model has it.
    #[must_use]
    pub fn model(self) -> Option<Frame> {
        match self {
            Self::Icrs => Some(Frame::Icrs),
            Self::Galactic => Some(Frame::Galactic),
            Self::Supergalactic => Some(Frame::Supergalactic),
            Self::Fk5J2000 => Some(Frame::Fk5J2000),
            Self::Fk4B1950 | Self::EquatorialEquinoxUnstated => None,
        }
    }
}

/// A distance along the line of sight as published: a redshift, a recession velocity or a
/// distance.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Radial {
    Redshift(Cited),
    Velocity(Cited),
    Distance(Cited),
}

impl Radial {
    #[must_use]
    pub fn cited(&self) -> &Cited {
        match self {
            Self::Redshift(c) | Self::Velocity(c) | Self::Distance(c) => c,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Redshift(_) => "redshift",
            Self::Velocity(_) => "velocity",
            Self::Distance(_) => "distance",
        }
    }

    fn check(&self) -> Result<(), String> {
        let unit = self.cited().unit();
        let ok = match self {
            Self::Redshift(_) => unit == Unit::Dimensionless,
            Self::Velocity(_) => unit.dimension() == Dimension::Velocity,
            Self::Distance(_) => is_length(unit),
        };
        if ok {
            Ok(())
        } else {
            Err(format!("a {} cannot be in {unit:?}", self.name()))
        }
    }
}

/// Whether `unit` measures a length, plain or in h⁻¹ Mpc.
pub(crate) fn is_length(unit: Unit) -> bool {
    matches!(
        unit.dimension(),
        Dimension::Length | Dimension::HubbleScaledLength
    )
}

/// A radial value as written: exactly one of its three fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRadial {
    #[serde(default)]
    redshift: Option<Cited>,
    #[serde(default)]
    velocity: Option<Cited>,
    #[serde(default)]
    distance: Option<Cited>,
}

fn radial(
    redshift: Option<Cited>,
    velocity: Option<Cited>,
    distance: Option<Cited>,
) -> Result<Option<Radial>, String> {
    let radial = match (redshift, velocity, distance) {
        (None, None, None) => None,
        (Some(z), None, None) => Some(Radial::Redshift(z)),
        (None, Some(v), None) => Some(Radial::Velocity(v)),
        (None, None, Some(d)) => Some(Radial::Distance(d)),
        _ => return Err("give one of redshift, velocity and distance".to_owned()),
    };
    radial.as_ref().map(Radial::check).transpose()?;
    Ok(radial)
}

impl<'de> Deserialize<'de> for Radial {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawRadial::deserialize(deserializer)?;
        radial(raw.redshift, raw.velocity, raw.distance)
            .and_then(|r| r.ok_or_else(|| "give one of redshift, velocity and distance".into()))
            .map_err(serde::de::Error::custom)
    }
}

/// A position as published, in a named frame.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Coordinates {
    /// Cartesian coordinates, all three in one unit: a length, h⁻¹ Mpc, or a velocity standing
    /// for a distance as in the Cosmicflows papers.
    Cartesian {
        frame: StructureFrame,
        x: Cited,
        y: Cited,
        z: Cited,
    },
    /// A longitude and latitude, with the distance along the line of sight where published. At a
    /// pole the longitude is undefined and left out.
    Spherical {
        frame: StructureFrame,
        lon: Option<Cited>,
        lat: Cited,
        radial: Option<Radial>,
    },
    /// The observer, at the origin of every frame: no number to cite.
    Observer,
}

/// Coordinates as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCoordinates {
    #[serde(default)]
    frame: Option<StructureFrame>,
    #[serde(default)]
    observer: bool,
    #[serde(default)]
    x: Option<Cited>,
    #[serde(default)]
    y: Option<Cited>,
    #[serde(default)]
    z: Option<Cited>,
    #[serde(default)]
    lon: Option<Cited>,
    #[serde(default)]
    lat: Option<Cited>,
    #[serde(default)]
    redshift: Option<Cited>,
    #[serde(default)]
    velocity: Option<Cited>,
    #[serde(default)]
    distance: Option<Cited>,
}

pub(crate) fn is_angle(c: &Cited) -> bool {
    c.unit().dimension() == Dimension::Angle
}

impl TryFrom<RawCoordinates> for Coordinates {
    type Error = String;
    fn try_from(raw: RawCoordinates) -> Result<Self, String> {
        let cartesian = raw.x.is_some() || raw.y.is_some() || raw.z.is_some();
        let spherical = raw.lon.is_some() || raw.lat.is_some();
        let radial = radial(raw.redshift, raw.velocity, raw.distance)?;
        if raw.observer {
            return if cartesian || spherical || radial.is_some() || raw.frame.is_some() {
                Err("the observer takes no frame or coordinates".to_owned())
            } else {
                Ok(Self::Observer)
            };
        }
        let frame = raw.frame.ok_or("coordinates need a frame")?;
        match (raw.x, raw.y, raw.z, raw.lon, raw.lat) {
            (Some(x), Some(y), Some(z), None, None) if radial.is_none() => {
                let unit = x.unit();
                let fits = is_length(unit) || unit.dimension() == Dimension::Velocity;
                if !fits || y.unit() != unit || z.unit() != unit {
                    return Err(
                        "x, y and z share one unit of length, h^-1 Mpc or velocity".to_owned()
                    );
                }
                Ok(Self::Cartesian { frame, x, y, z })
            }
            (None, None, None, lon, Some(lat)) => {
                let at_pole = (lat.value().abs() - 90.0).abs() < 1e-9;
                if !is_angle(&lat) || lon.as_ref().is_some_and(|l| !is_angle(l)) {
                    return Err("lon and lat are angles".to_owned());
                }
                if lon.is_none() && !at_pole {
                    return Err("lon may be left out only at a pole".to_owned());
                }
                Ok(Self::Spherical {
                    frame,
                    lon,
                    lat,
                    radial,
                })
            }
            _ => Err(
                "give x, y and z; or lat with lon (and one of redshift, velocity, distance); \
                 or observer: true"
                    .to_owned(),
            ),
        }
    }
}

impl<'de> Deserialize<'de> for Coordinates {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawCoordinates::deserialize(deserializer)?;
        Coordinates::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl Coordinates {
    /// The frame, or none for the observer, who is at the origin of all.
    #[must_use]
    pub fn frame(&self) -> Option<StructureFrame> {
        match self {
            Self::Cartesian { frame, .. } | Self::Spherical { frame, .. } => Some(*frame),
            Self::Observer => None,
        }
    }

    /// Each cited number with its field name.
    pub(crate) fn cited(&self) -> Vec<(String, &Cited)> {
        match self {
            Self::Cartesian { x, y, z, .. } => {
                vec![("x".into(), x), ("y".into(), y), ("z".into(), z)]
            }
            Self::Spherical {
                lon, lat, radial, ..
            } => {
                let mut out: Vec<(String, &Cited)> =
                    lon.iter().map(|l| ("lon".into(), l)).collect();
                out.push(("lat".into(), lat));
                out.extend(radial.iter().map(|r| (r.name().to_owned(), r.cited())));
                out
            }
            Self::Observer => vec![],
        }
    }
}
