//! A structure's geometry beyond a point: a primitive solid, a member list or a sky region, each
//! one of univ-model's geometry kinds. Each is read through a raw struct rather than a tagged
//! enum, so a refusal keeps the path to the field.

use serde::{Deserialize, Serialize};

use super::cited::Cited;
use super::geometry::{Coordinates, Radial, StructureFrame, is_angle, is_length};
use super::members::Members;

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
}

/// The univ-model geometry kinds a structure file may give, beside the point of its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Primitive,
    Members,
    SkyRegion,
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
}

impl TryFrom<RawGeometry> for StructureGeometry {
    type Error = String;
    fn try_from(raw: RawGeometry) -> Result<Self, String> {
        match (raw.kind, raw.primitive, raw.members, raw.sky_region) {
            (Kind::Primitive, Some(p), None, None) => Ok(Self::Primitive(Box::new(p))),
            (Kind::Members, None, Some(m), None) => Ok(Self::Members(m)),
            (Kind::SkyRegion, None, None, Some(s)) => Ok(Self::SkyRegion(Box::new(s))),
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
            Self::Primitive(_) => "primitive",
            Self::Members(_) => "members",
            Self::SkyRegion(_) => "sky_region",
        }
    }

    /// The frame the geometry is published in, or none for a sphere about the observer.
    #[must_use]
    pub fn frame(&self) -> Option<StructureFrame> {
        match self {
            Self::Primitive(p) => p.centre.frame(),
            Self::Members(m) => Some(m.frame),
            Self::SkyRegion(s) => Some(s.frame),
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
                let mut out = vec![];
                for (name, pair) in [("lon", &s.lon), ("lat", &s.lat)] {
                    for (i, c) in pair.iter().enumerate() {
                        out.push((format!("sky_region.{name}[{i}]"), c));
                    }
                }
                for (name, r) in [("near", &s.near), ("far", &s.far)] {
                    out.push((format!("sky_region.{name}.{}", r.name()), r.cited()));
                }
                out
            }
        }
    }
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

/// A longitude range from `lon[0]` eastward to `lon[1]` (wrapping through 0 where `lon[0]` is
/// larger), a latitude range and radial bounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSkyBox")]
pub struct SkyBox {
    pub frame: StructureFrame,
    pub lon: [Cited; 2],
    pub lat: [Cited; 2],
    pub near: Radial,
    pub far: Radial,
}

/// A sky box as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSkyBox {
    frame: StructureFrame,
    lon: [Cited; 2],
    lat: [Cited; 2],
    near: Radial,
    far: Radial,
}

impl TryFrom<RawSkyBox> for SkyBox {
    type Error = String;
    fn try_from(raw: RawSkyBox) -> Result<Self, String> {
        if !raw.lon.iter().chain(&raw.lat).all(is_angle) {
            return Err("lon and lat bounds are angles".into());
        }
        if raw.lat[0].value() > raw.lat[1].value() {
            return Err("lat runs from the lower bound to the upper".into());
        }
        if raw.near.name() != raw.far.name() || raw.near.cited().unit() != raw.far.cited().unit() {
            return Err("near and far are the same measure in the same unit".into());
        }
        if raw.near.cited().value() > raw.far.cited().value() {
            return Err("near lies beyond far".into());
        }
        Ok(Self {
            frame: raw.frame,
            lon: raw.lon,
            lat: raw.lat,
            near: raw.near,
            far: raw.far,
        })
    }
}
