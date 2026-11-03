//! Each researched structure's placed geometry as a univ-model shape in the ICRS about the
//! barycentre, in its scale domain's unit, beside the geometry its file gives, with how it was
//! derived: the cited values used, the frames converted through and how its distance was found.
//!
//! The shape is placed as [`super::place`] places the structure's centre and bounds, by the same
//! functions: a member list whose members are all placed is its member list (`crate::crossmatch`);
//! one whose members are not is its published position, as is a structure with a position
//! alone; a sphere is its placed centre and its radius rescaled to the release cosmology; a sky
//! region is its `HEALPix` cells ([`super::sky::box_cells`]) between its near and far distances.

use serde_json::{Value as Json, json};
use univ_model::derive::Release;
use univ_model::geometry::{Geometry, Member, Primitive, Shape};
use univ_model::position::{Frame, Vec3};
use univ_model::redshift::RedshiftFrame;
use univ_model::units::{Dimension, Unit};

use super::objects::snake;
use super::place::{assumed, length, place, radial};
use super::pointers::{self, Pointed};
use super::sky::{REGION_ORDER, box_cells};
use crate::crossmatch::MemberPlacement;
use crate::source::ScaleDomain;
use crate::structures::{Coordinates, Members, Radial, SphereSize, Structure, StructureGeometry};
use univ_model::redshift::CmbCorrection;

/// A structure's shape as placed, and the JSON of its derivation.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedShape {
    pub shape: Shape,
    pub derivation: Json,
}

/// What a structure is placed under: the release, the run's dipole correction and its name.
pub struct Placing<'a> {
    pub release: &'a Release,
    pub correction: &'a CmbCorrection,
    /// The dipole model's name, such as `planck2018`.
    pub dipole: String,
}

/// The frames a position in `frame` is converted through to the ICRS.
#[must_use]
pub fn frame_chain(frame: Option<Frame>) -> Vec<String> {
    let to_icrs = "fk5_j2000 -> icrs".to_owned();
    match frame {
        None | Some(Frame::Icrs) => vec![],
        Some(Frame::Fk5J2000) => vec![to_icrs],
        Some(Frame::Fk4B1950) => vec![
            "fk4_b1950 -> fk5_j2000 (E-terms of aberration removed, as ERFA fk45z, zero proper \
             motion)"
                .to_owned(),
            to_icrs,
        ],
        Some(Frame::EquatorialEquinoxUnstated) => vec![
            "equatorial_equinox_unstated -> fk5_j2000 (the equinox read as J2000; the bounds \
             grow by the B1950 to J2000 angle)"
                .to_owned(),
            to_icrs,
        ],
        Some(Frame::Galactic) => vec!["galactic -> icrs (Hipparcos, ESA 1997)".to_owned()],
        Some(Frame::Supergalactic) => {
            vec!["supergalactic -> icrs (Lahav et al. 2000 pole and origin)".to_owned()]
        }
    }
}

/// How a published length becomes Mpc of the release cosmology, as [`super::place`] takes it.
fn length_method(c: &crate::structures::Cited) -> String {
    match assumed(c) {
        Some(a) => format!(
            "the length as published, rescaled from H0 = {} km/s/Mpc to the release's by the \
             ratio of Hubble constants",
            a.hubble_constant
        ),
        None => "the length as published, which states no cosmology".to_owned(),
    }
}

/// How a published radial value becomes a distance, `frame` the redshift frame its paper gives.
fn radial_distance(r: &Radial, frame: Option<RedshiftFrame>, dipole: &str) -> Json {
    let (what, z) = match r {
        Radial::Redshift(_) => ("redshift", "the redshift"),
        Radial::Velocity(_) => ("velocity cz", "the redshift z = cz / c"),
        Radial::Distance(c) => {
            return json!({"from": "the published distance", "method": length_method(c)});
        }
    };
    if frame == Some(RedshiftFrame::Heliocentric) {
        return json!({
            "from": format!("the heliocentric {what} as published"),
            "method": format!(
                "comoving distance of the CMB-frame redshift, {z} moved from heliocentric with \
                 the {dipole} dipole toward the published direction, under the release cosmology"
            ),
        });
    }
    let frame = frame.map_or_else(|| "its frame not stated".to_owned(), |f| snake(&f));
    json!({
        "from": format!("the {what} as published ({frame})"),
        "method": format!(
            "comoving distance of {z}, taken as published and not moved, under the release \
             cosmology"
        ),
    })
}

/// How a published position's distance was found.
fn coordinates_distance(at: &Coordinates, dipole: &str) -> Json {
    match at {
        Coordinates::Observer => json!({"from": "the observer", "method": "the origin"}),
        Coordinates::Cartesian { x, .. } if x.unit().dimension() == Dimension::Velocity => json!({
            "from": "the published Cartesian velocities",
            "method": "comoving distance of the redshift v / c of their magnitude, taken as \
                       published, under the release cosmology, along their direction",
        }),
        Coordinates::Cartesian { x, .. } => json!({
            "from": "the published Cartesian lengths",
            "method": length_method(x),
        }),
        Coordinates::Spherical {
            radial: r,
            redshift_frame,
            ..
        } => r.as_ref().map_or_else(
            || json!({"from": "none", "method": "the position gives no distance"}),
            |r| radial_distance(r, *redshift_frame, dipole),
        ),
    }
}

fn derivation(from: &[Pointed<'_>], chain: &[String], distance: &Json) -> Json {
    let from: Vec<&str> = from.iter().map(|(p, _)| p.as_str()).collect();
    json!({"from": from, "frame_chain": chain, "distance": distance, "cosmology": "release"})
}

/// A published position as a point, with its derivation.
fn point(s: &Structure, at: &Coordinates, p: &Placing<'_>) -> Result<(Geometry, Json), String> {
    let (centre, _) = place(at, p.release, p.correction)?;
    let derived = derivation(
        &pointers::position(s),
        &frame_chain(at.frame()),
        &coordinates_distance(at, &p.dipole),
    );
    Ok((Geometry::Point { at: centre }, derived))
}

/// A member list placed whole, with its derivation.
fn members(m: &Members, placed: &MemberPlacement, shape: &Shape, dipole: &str) -> (Geometry, Json) {
    let mut from = vec!["/geometry/Members".to_owned()];
    for key in crate::crossmatch::STRUCTURE_REDSHIFTS {
        let named = format!("the structure's value {key}");
        if placed.from_file.iter().any(|f| f.redshift_from == named) {
            from.push(pointers::value(key));
        }
    }
    let derived = json!({
        "from": from,
        "frame_chain": frame_chain(Some(m.frame)),
        "distance": {
            "from": "each member's matched object, or, for a member no object carries, its own \
                     sky position and redshift in the structure file",
            "method": format!(
                "a matched member at its object's derived position; any other member a tracer \
                 at the comoving distance of its redshift, taken as heliocentric and moved to \
                 the CMB frame with the {dipole} dipole toward it, under the release cosmology"
            ),
        },
        "cosmology": "release",
    });
    (shape.geometry.clone(), derived)
}

/// `geometry`, placed in Mpc, in `per_mpc` units of the scale domain per Mpc.
fn in_units(geometry: Geometry, per_mpc: f64) -> Geometry {
    if (per_mpc - 1.0).abs() < f64::EPSILON {
        return geometry;
    }
    let s = |v: Vec3| v.map(|c| c * per_mpc);
    match geometry {
        Geometry::Point { at } => Geometry::Point { at: s(at) },
        Geometry::Primitive {
            primitive: Primitive::Sphere { centre, radius },
        } => Geometry::Primitive {
            primitive: Primitive::Sphere {
                centre: s(centre),
                radius: radius * per_mpc,
            },
        },
        Geometry::Members { members } => Geometry::Members {
            members: members
                .into_iter()
                .map(|m| match m {
                    Member::Object { id, at } => Member::Object { id, at: s(at) },
                    Member::Tracer { at, kind } => Member::Tracer { at: s(at), kind },
                })
                .collect(),
        },
        Geometry::SkyRegion {
            order,
            cells,
            near,
            far,
        } => Geometry::SkyRegion {
            order,
            cells,
            near: near * per_mpc,
            far: far * per_mpc,
        },
        other => other,
    }
}

/// The shape of `s` as placed under `p`, its member list `placed` where it has one, and its
/// derivation; or why it cannot be placed, as [`super::index_bounds`] says.
///
/// # Errors
///
/// Fails, saying why, for a structure that cannot be placed, or whose shape cannot stand.
pub fn placed_shape(
    s: &Structure,
    placed: Option<&MemberPlacement>,
    p: &Placing<'_>,
) -> Result<PlacedShape, String> {
    let no_position = || "it publishes no position or geometry".to_owned();
    let (geometry, derivation) = match &s.geometry {
        Some(StructureGeometry::Members(m)) => match placed {
            Some(pl) => match &pl.shape {
                Some(shape) => members(m, pl, shape, &p.dipole),
                None => point(s, s.position.as_ref().ok_or_else(no_position)?, p)?,
            },
            None => return Err("its member list was not resolved".to_owned()),
        },
        Some(StructureGeometry::Primitive(sphere)) => {
            let (centre, _) = place(&sphere.centre, p.release, p.correction)?;
            let radius = match &sphere.size {
                SphereSize::Radius(r) => length(r, p.release)?,
                SphereSize::Diameter(d) => length(d, p.release)? / 2.0,
            };
            let geometry = Geometry::Primitive {
                primitive: Primitive::Sphere { centre, radius },
            };
            let from = pointers::geometry(s.geometry.as_ref().ok_or_else(no_position)?);
            let distance = coordinates_distance(&sphere.centre, &p.dipole);
            (
                geometry,
                derivation(&from, &frame_chain(sphere.centre.frame()), &distance),
            )
        }
        Some(StructureGeometry::SkyRegion(b)) => {
            let (near, far) = (radial(&b.near, p.release)?, radial(&b.far, p.release)?);
            let geometry = Geometry::SkyRegion {
                order: REGION_ORDER,
                cells: box_cells(b, REGION_ORDER)?,
                near: near.min(far),
                far: near.max(far),
            };
            let from = pointers::geometry(s.geometry.as_ref().ok_or_else(no_position)?);
            let near = radial_distance(&b.near, None, &p.dipole);
            let distance = json!({
                "from": format!(
                    "the published near and far bounds, each {}",
                    near["from"].as_str().unwrap_or_default()
                ),
                "method": format!(
                    "{}, for each bound; the cells of HEALPix order {REGION_ORDER} whose centres \
                     fall in the published longitude and latitude box",
                    near["method"].as_str().unwrap_or_default()
                ),
            });
            (
                geometry,
                derivation(&from, &frame_chain(Some(b.frame)), &distance),
            )
        }
        None => point(s, s.position.as_ref().ok_or_else(no_position)?, p)?,
    };
    let (unit, per_mpc) = if s.scale_domain == ScaleDomain::Stars {
        (Unit::Parsec, 1e6)
    } else {
        (Unit::Megaparsec, 1.0)
    };
    let shape = Shape::new(Frame::Icrs, unit, in_units(geometry, per_mpc))
        .map_err(|e| format!("its shape cannot stand: {e}"))?;
    Ok(PlacedShape { shape, derivation })
}
