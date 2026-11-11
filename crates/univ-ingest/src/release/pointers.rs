//! JSON pointers (RFC 6901) into a structure's `record`, the JSON of its file as
//! `structures.record` holds it, naming each cited value a derived value comes from.

use crate::structures::{Cited, Coordinates, Radial, Structure, StructureGeometry};

/// A cited value with the pointer to it in the record.
pub type Pointed<'a> = (String, &'a Cited);

/// `segment` escaped for a JSON pointer: `~` as `~0`, `/` as `~1`.
#[must_use]
pub fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// The cited values of a radial value at `prefix`, such as `/geometry/SkyRegion/near`.
fn radial<'a>(prefix: &str, r: &'a Radial) -> Pointed<'a> {
    (format!("{prefix}/{}", r.name()), r.cited())
}

/// The cited values of coordinates at `prefix`, such as `/position`.
#[must_use]
pub fn coordinates<'a>(prefix: &str, c: &'a Coordinates) -> Vec<Pointed<'a>> {
    match c {
        Coordinates::Cartesian { x, y, z, .. } => vec![
            (format!("{prefix}/x"), x),
            (format!("{prefix}/y"), y),
            (format!("{prefix}/z"), z),
        ],
        Coordinates::Spherical {
            lon,
            lat,
            radial: r,
            ..
        } => {
            let mut out: Vec<Pointed<'a>> =
                lon.iter().map(|l| (format!("{prefix}/lon"), l)).collect();
            out.push((format!("{prefix}/lat"), lat));
            out.extend(r.iter().map(|r| radial(&format!("{prefix}/radial"), r)));
            out
        }
        Coordinates::Observer => vec![],
    }
}

/// Where a sky region's paper gives it as a lower limit on the structure's extent.
pub const AT_LEAST: &str = "/geometry/SkyRegion/at_least";

/// Each cited bound of named pairs, at `{at}/{name}/{0 or 1}`.
fn indexed<'a>(
    at: &str,
    pairs: impl IntoIterator<Item = (&'static str, &'a [Cited; 2])>,
) -> Vec<Pointed<'a>> {
    pairs
        .into_iter()
        .flat_map(|(name, pair)| {
            pair.iter()
                .enumerate()
                .map(move |(i, c)| (format!("{at}/{name}/{i}"), c))
        })
        .collect()
}

/// The cited values of a primitive or a sky region; none for a member list, whose rows cite its
/// table through its source.
#[must_use]
pub fn geometry(g: &StructureGeometry) -> Vec<Pointed<'_>> {
    match g {
        StructureGeometry::Primitive(p) => {
            let mut out = coordinates("/geometry/Primitive/centre", &p.centre);
            let (name, size) = match &p.size {
                crate::structures::SphereSize::Radius(c) => ("radius", c),
                crate::structures::SphereSize::Diameter(c) => ("diameter", c),
            };
            out.push((format!("/geometry/Primitive/size/{name}"), size));
            out
        }
        StructureGeometry::SkyRegion(b) => {
            let at = "/geometry/SkyRegion";
            let mut out = indexed(at, b.sides.pairs());
            out.push(radial(&format!("{at}/near"), &b.near));
            out.push(radial(&format!("{at}/far"), &b.far));
            out
        }
        StructureGeometry::AxisBox(b) => indexed("/geometry/AxisBox", b.pairs()),
        StructureGeometry::LogSpiral(l) => {
            let at = "/geometry/LogSpiral";
            let mut out = indexed(at, l.pairs());
            out.extend(l.scalars().map(|(name, c)| (format!("{at}/{name}"), c)));
            out
        }
        StructureGeometry::Members(_) => vec![],
    }
}

/// The cited values of the structure's published position, at `/position`.
#[must_use]
pub fn position(s: &Structure) -> Vec<Pointed<'_>> {
    s.position
        .as_ref()
        .map(|p| coordinates("/position", p))
        .unwrap_or_default()
}

/// The pointer to the structure value `name`.
#[must_use]
pub fn value(name: &str) -> String {
    format!("/values/{}", escape(name))
}
