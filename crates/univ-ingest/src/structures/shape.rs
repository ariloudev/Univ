//! A structure's geometry beyond a point: a primitive solid, a box along its frame's axes, a
//! member list, a sky region or a log spiral, each one of univ-model's geometry kinds. Each is read through a raw struct rather than a tagged
//! enum, so a refusal keeps the path to the field.

use serde::{Deserialize, Serialize};

use super::citation::Citation;
use super::cited::Cited;
use super::geometry::{Coordinates, Radial, is_angle, is_length};
use super::members::Members;
use super::spiral::LogSpiral;
use univ_model::position::Frame;
use univ_model::units::Unit;

/// A structure's geometry: one of univ-model's geometry kinds, as published. A structure with
/// only a position has univ-model's `point` geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGeometry")]
pub enum StructureGeometry {
    /// A solid of a simple form; univ-model's `primitive`.
    Primitive(Box<StructurePrimitive>),
    /// The members of the structure, from a table or figure; univ-model's `members`.
    Members(Members),
    /// A region of the sky between two radial bounds, given by its published longitude and
    /// latitude ranges; univ-model's `sky_region`, whose `HEALPix` cells the pipeline computes.
    SkyRegion(Box<SkyBox>),
    /// A box along the Cartesian axes of its frame, given by the published range along each;
    /// univ-model's `primitive`, an oriented box.
    AxisBox(Box<AxisBox>),
    /// A spiral arm as its published log-spiral parameters; univ-model's `path`, computed at
    /// each degree of azimuth in the galactic frame.
    LogSpiral(Box<LogSpiral>),
}

/// The univ-model geometry kinds a structure file may give, beside the point of its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Primitive,
    Members,
    SkyRegion,
    Box,
    LogSpiral,
}

/// A geometry as written: its kind, and the one field that kind takes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGeometry {
    kind: Kind,
    #[serde(default)]
    primitive: Option<StructurePrimitive>,
    #[serde(default)]
    members: Option<Members>,
    #[serde(default)]
    sky_region: Option<SkyBox>,
    #[serde(default, rename = "box")]
    axis_box: Option<AxisBox>,
    #[serde(default)]
    log_spiral: Option<LogSpiral>,
}

impl TryFrom<RawGeometry> for StructureGeometry {
    type Error = String;
    fn try_from(raw: RawGeometry) -> Result<Self, String> {
        match (
            raw.kind,
            raw.primitive,
            raw.members,
            raw.sky_region,
            raw.axis_box,
            raw.log_spiral,
        ) {
            (Kind::Primitive, Some(p), None, None, None, None) => Ok(Self::Primitive(Box::new(p))),
            (Kind::Members, None, Some(m), None, None, None) => Ok(Self::Members(m)),
            (Kind::SkyRegion, None, None, Some(s), None, None) => Ok(Self::SkyRegion(Box::new(s))),
            (Kind::Box, None, None, None, Some(b), None) => Ok(Self::AxisBox(Box::new(b))),
            (Kind::LogSpiral, None, None, None, None, Some(l)) => Ok(Self::LogSpiral(Box::new(l))),
            (kind, ..) => Err(format!(
                "a {kind:?} geometry takes exactly the field named after its kind"
            )),
        }
    }
}

impl StructureGeometry {
    /// The univ-model geometry kind, as univ-model tags it.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Primitive(_) | Self::AxisBox(_) => "primitive",
            Self::Members(_) => "members",
            Self::SkyRegion(_) => "sky_region",
            Self::LogSpiral(_) => "path",
        }
    }

    /// The frame the geometry is published in, or none for a sphere about the observer.
    #[must_use]
    pub fn frame(&self) -> Option<Frame> {
        match self {
            Self::Primitive(p) => p.centre.frame(),
            Self::Members(m) => Some(m.frame),
            Self::SkyRegion(s) => Some(s.frame),
            Self::AxisBox(b) => Some(b.frame),
            Self::LogSpiral(_) => Some(Frame::Galactic),
        }
    }

    /// Each cited number with its path below the geometry.
    pub(crate) fn cited(&self) -> Vec<(String, &Cited)> {
        match self {
            Self::Primitive(p) => {
                let mut out: Vec<(String, &Cited)> = p
                    .centre
                    .cited()
                    .into_iter()
                    .map(|(f, c)| (format!("primitive.centre.{f}"), c))
                    .collect();
                out.push((format!("primitive.{}", p.size.name()), p.size.cited()));
                out
            }
            Self::Members(_) => vec![],
            Self::SkyRegion(s) => {
                let mut out = indexed("sky_region", s.sides.pairs());
                for (name, r) in [("near", &s.near), ("far", &s.far)] {
                    out.push((format!("sky_region.{name}.{}", r.name()), r.cited()));
                }
                out
            }
            Self::AxisBox(b) => indexed("box", b.pairs()),
            Self::LogSpiral(l) => l
                .pairs()
                .into_iter()
                .map(|(name, c)| (format!("log_spiral.{name}"), c))
                .collect(),
        }
    }
}

/// Each cited bound of named pairs, with its field path `{at}.{name}[{0 or 1}]`.
fn indexed<'a>(
    at: &str,
    pairs: impl IntoIterator<Item = (&'static str, &'a [Cited; 2])>,
) -> Vec<(String, &'a Cited)> {
    pairs
        .into_iter()
        .flat_map(|(name, pair)| {
            pair.iter()
                .enumerate()
                .map(move |(i, c)| (format!("{at}.{name}[{i}]"), c))
        })
        .collect()
}

/// The size of a sphere as published: its radius or its diameter.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SphereSize {
    Radius(Cited),
    Diameter(Cited),
}

impl SphereSize {
    #[must_use]
    pub fn cited(&self) -> &Cited {
        match self {
            Self::Radius(c) | Self::Diameter(c) => c,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Radius(_) => "radius",
            Self::Diameter(_) => "diameter",
        }
    }
}

/// A sphere, the one primitive form the structure files use so far.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPrimitive")]
pub struct StructurePrimitive {
    pub centre: Coordinates,
    pub size: SphereSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Form {
    Sphere,
}

/// A primitive as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPrimitive {
    form: Form,
    centre: Coordinates,
    #[serde(default)]
    radius: Option<Cited>,
    #[serde(default)]
    diameter: Option<Cited>,
}

impl TryFrom<RawPrimitive> for StructurePrimitive {
    type Error = String;
    fn try_from(raw: RawPrimitive) -> Result<Self, String> {
        let Form::Sphere = raw.form;
        let size = match (raw.radius, raw.diameter) {
            (Some(r), None) => SphereSize::Radius(r),
            (None, Some(d)) => SphereSize::Diameter(d),
            _ => return Err("a sphere takes one of radius and diameter".into()),
        };
        if !is_length(size.cited().unit()) {
            return Err("a sphere's radius or diameter is a length".into());
        }
        Ok(Self {
            centre: raw.centre,
            size,
        })
    }
}

/// A region of the sky between radial bounds: its sides as its paper gives them, and, where
/// the paper says the structure extends at least over it, where it says so.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSkyBox")]
pub struct SkyBox {
    pub frame: Frame,
    #[serde(flatten)]
    pub sides: Sides,
    pub near: Radial,
    pub far: Radial,
    /// The sentence in which the paper gives the region as a lower limit on the structure's
    /// extent, such as "at least 25° × 20°".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least: Option<Citation>,
}

/// The sides of a sky region as published.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Sides {
    /// A longitude range from `lon[0]` eastward to `lon[1]` (wrapping through 0 where `lon[0]`
    /// is larger) and a latitude range.
    Ranges { lon: [Cited; 2], lat: [Cited; 2] },
    /// A centre, longitude and latitude, and the width and height on the sky about it: the
    /// width an angle along the sky at the centre, not a span of longitude.
    Centred {
        centre: [Cited; 2],
        size: [Cited; 2],
    },
}

impl Sides {
    /// Its two pairs of cited angles, each with its field name.
    #[must_use]
    pub fn pairs(&self) -> [(&'static str, &[Cited; 2]); 2] {
        match self {
            Self::Ranges { lon, lat } => [("lon", lon), ("lat", lat)],
            Self::Centred { centre, size } => [("centre", centre), ("size", size)],
        }
    }

    /// How the region's cells are chosen from these sides, for its derivation.
    #[must_use]
    pub fn described(&self) -> &'static str {
        match self {
            Self::Ranges { .. } => "the published longitude and latitude box",
            Self::Centred { .. } => {
                "the box of the published width and height about the published centre, its \
                 latitude the height about the centre's and its longitude the half-width over \
                 the cosine of the centre's latitude either side of the centre's"
            }
        }
    }
}

/// A sky box as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSkyBox {
    frame: Frame,
    #[serde(default)]
    lon: Option<[Cited; 2]>,
    #[serde(default)]
    lat: Option<[Cited; 2]>,
    #[serde(default)]
    centre: Option<[Cited; 2]>,
    #[serde(default)]
    size: Option<[Cited; 2]>,
    near: Radial,
    far: Radial,
    #[serde(default)]
    at_least: Option<Citation>,
}

impl TryFrom<RawSkyBox> for SkyBox {
    type Error = String;
    fn try_from(raw: RawSkyBox) -> Result<Self, String> {
        let sides = match (raw.lon, raw.lat, raw.centre, raw.size) {
            (Some(lon), Some(lat), None, None) => {
                if !lon.iter().chain(&lat).all(is_angle) {
                    return Err("lon and lat bounds are angles".into());
                }
                if lat[0].value() > lat[1].value() {
                    return Err("lat runs from the lower bound to the upper".into());
                }
                Sides::Ranges { lon, lat }
            }
            (None, None, Some(centre), Some(size)) => {
                if !centre.iter().chain(&size).all(is_angle) {
                    return Err("centre and size are angles".into());
                }
                let degrees =
                    |c: &Cited| c.value() * c.unit().factor_to(Unit::Degree).unwrap_or(f64::NAN);
                let (lon, lat, height) =
                    (degrees(&centre[0]), degrees(&centre[1]), degrees(&size[1]));
                if !(0.0..360.0).contains(&lon) {
                    return Err("the centre's longitude lies in 0° to 360°".into());
                }
                // A centre within 90° of the equator whose box stays clear of the poles.
                if size.iter().any(|s| s.value() <= 0.0) || (lat.abs() + height / 2.0) >= 90.0 {
                    return Err(
                        "size is a positive width and height whose box stays clear of the poles"
                            .into(),
                    );
                }
                Sides::Centred { centre, size }
            }
            _ => return Err("a sky region gives lon and lat, or centre and size".into()),
        };
        if raw.near.name() != raw.far.name() || raw.near.cited().unit() != raw.far.cited().unit() {
            return Err("near and far are the same measure in the same unit".into());
        }
        if raw.near.cited().value() > raw.far.cited().value() {
            return Err("near lies beyond far".into());
        }
        Ok(Self {
            frame: raw.frame,
            sides,
            near: raw.near,
            far: raw.far,
            at_least: raw.at_least,
        })
    }
}

/// A box along the Cartesian axes of `frame`, such as supergalactic SGX, SGY and SGZ: the range
/// its paper gives along each, from the lower bound to the upper, each a length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawAxisBox")]
pub struct AxisBox {
    pub frame: Frame,
    pub x: [Cited; 2],
    pub y: [Cited; 2],
    pub z: [Cited; 2],
}

impl AxisBox {
    /// Its three ranges, each a pair of cited bounds with its axis's name.
    #[must_use]
    pub fn pairs(&self) -> [(&'static str, &[Cited; 2]); 3] {
        [("x", &self.x), ("y", &self.y), ("z", &self.z)]
    }
}

/// A box as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAxisBox {
    frame: Frame,
    x: [Cited; 2],
    y: [Cited; 2],
    z: [Cited; 2],
}

impl TryFrom<RawAxisBox> for AxisBox {
    type Error = String;
    fn try_from(raw: RawAxisBox) -> Result<Self, String> {
        let b = Self {
            frame: raw.frame,
            x: raw.x,
            y: raw.y,
            z: raw.z,
        };
        for (name, [low, high]) in b.pairs() {
            if !is_length(low.unit()) || low.unit() != high.unit() {
                return Err(format!("the box's {name} bounds are lengths in one unit"));
            }
            if low.value() >= high.value() {
                return Err(format!(
                    "the box's {name} runs from the lower bound to the upper"
                ));
            }
        }
        Ok(b)
    }
}
