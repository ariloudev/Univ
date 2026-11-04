//! Each researched structure's placed geometry as a univ-model shape about the barycentre, in its
//! scale domain's unit, beside the geometry its file gives, with how it was derived: the cited
//! values used, the frames converted through and how its distance was found.
//!
//! The shape is placed as [`super::place`] places the structure's centre and bounds, by the same
//! functions, in the ICRS: a member list whose members are all placed is its member list
//! (`crate::crossmatch`); one whose members are not is its published position, as is a structure
//! with a position alone; a sphere is its placed centre and its radius rescaled to the release
//! cosmology; a sky region is its `HEALPix` cells ([`super::sky::box_cells`]) between its near and
//! far distances. An outline its grid test admitted is placed instead as its region, in the
//! grid's frame ([`super::outline`]).
//!
//! A frame move is cited as an object's derivation cites it: univ-model's chain steps.

use univ_model::api::structures::{Derivation, DistanceDerivation, Extents};
use univ_model::derive::{Release, Step, Transformation};
use univ_model::geometry::{Geometry, Member, Primitive, Shape};
use univ_model::position::{Frame, Vec3, between};
use univ_model::redshift::{CmbCorrection, RedshiftFrame};
use univ_model::units::{Dimension, Unit};

use super::objects::snake;
use super::place::{assumed, km_s, length, place, radial, sphere_radius};
use super::pointers::{self, Pointed};
use super::sky::{REGION_ORDER, box_cells};
use crate::crossmatch::MemberPlacement;
use crate::source::ScaleDomain;
use crate::structures::{Coordinates, Members, Radial, Structure, StructureGeometry};
use crate::tiles::unit_of;

/// A structure's shape as placed, and its derivation.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedShape {
    pub shape: Shape,
    pub derivation: Derivation,
}

/// What a structure is placed under: the release, the run's dipole correction and its name.
pub struct Placing<'a> {
    pub release: &'a Release,
    pub correction: &'a CmbCorrection,
    /// The dipole model's name, such as `planck2018`.
    pub dipole: String,
}

/// The steps that move a position in `frame` to the ICRS, as an object's chain gives them: an
/// unstated equinox first taken as J2000, then the move to the ICRS; none from the ICRS.
#[must_use]
pub fn frame_chain(frame: Option<Frame>) -> Vec<Step> {
    let Some(frame) = frame.filter(|f| *f != Frame::Icrs) else {
        return Vec::new();
    };
    let mut chain = Vec::with_capacity(2);
    if let Some(systematic_degrees) = frame.systematic_uncertainty() {
        chain.push(Transformation::EquinoxAssumedJ2000 { systematic_degrees }.into());
    }
    chain.push(Transformation::ToIcrs { from: frame }.into());
    chain
}

fn distance(from: impl Into<String>, method: impl Into<String>) -> DistanceDerivation {
    DistanceDerivation {
        from: from.into(),
        method: method.into(),
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
fn radial_distance(r: &Radial, frame: Option<RedshiftFrame>, dipole: &str) -> DistanceDerivation {
    let (what, z) = match r {
        Radial::Redshift(_) => ("redshift", "the redshift"),
        Radial::Velocity(_) => ("velocity cz", "the redshift z = cz / c"),
        Radial::Distance(c) => return distance("the published distance", length_method(c)),
    };
    if frame == Some(RedshiftFrame::Heliocentric) {
        return distance(
            format!("the heliocentric {what} as published"),
            format!(
                "comoving distance of the CMB-frame redshift, {z} moved from heliocentric with \
                 the {dipole} dipole toward the published direction, under the release cosmology"
            ),
        );
    }
    let frame = frame.map_or_else(|| "its frame not stated".to_owned(), |f| snake(&f));
    distance(
        format!("the {what} as published ({frame})"),
        format!(
            "comoving distance of {z}, taken as published and not moved, under the release \
             cosmology"
        ),
    )
}

/// How a published position's distance was found.
fn coordinates_distance(at: &Coordinates, dipole: &str) -> DistanceDerivation {
    match at {
        Coordinates::Observer => distance("the observer", "the origin"),
        Coordinates::Cartesian { x, .. } if x.unit().dimension() == Dimension::Velocity => {
            distance(
                "the published Cartesian velocities",
                "comoving distance of the redshift v / c of their magnitude, taken as published, \
             under the release cosmology, along their direction",
            )
        }
        Coordinates::Cartesian { x, .. } => {
            distance("the published Cartesian lengths", length_method(x))
        }
        Coordinates::Spherical {
            radial: r,
            redshift_frame,
            ..
        } => r.as_ref().map_or_else(
            || distance("none", "the position gives no distance"),
            |r| radial_distance(r, *redshift_frame, dipole),
        ),
    }
}

/// A derivation under the release cosmology.
pub(super) fn derivation(
    from: Vec<String>,
    frame_chain: Vec<Step>,
    distance: DistanceDerivation,
) -> Derivation {
    Derivation {
        from,
        frame_chain,
        distance,
        cosmology: "release".to_owned(),
        grid: None,
        at_least: None,
        extents: None,
    }
}

/// How [`extents`] turns a published extent into a length.
const EXTENTS_METHOD: &str = "a velocity divided by the release Hubble constant, as a Hubble-flow \
     length; a length rescaled to the release cosmology; each laid along its frame's axis turned \
     into the ICRS";

/// The extents of `s` about its position, in `unit`: each axis of the position's frame turned
/// into the ICRS, as long as the published extent along it.
fn extents(s: &Structure, release: &Release, unit: Unit) -> Result<Option<Extents>, String> {
    let (Some(names), Some(frame)) = (&s.extents, s.position.as_ref().and_then(Coordinates::frame))
    else {
        return Ok(None);
    };
    let per_mpc = Unit::Megaparsec
        .factor_to(unit)
        .map_err(|e| e.to_string())?;
    let to_icrs = between(frame, Frame::Icrs);
    let mut axes = [[0.0; 3]; 3];
    for (k, name) in names.names().into_iter().enumerate() {
        let c = s
            .values
            .get(name)
            .ok_or_else(|| format!("its extents name {name}, which is not among its values"))?;
        let mpc = if c.unit().dimension() == Dimension::Velocity {
            km_s(c)? / release.cosmology().hubble_constant
        } else {
            length(c, release)?
        };
        let mut along = [0.0; 3];
        along[k] = 1.0;
        axes[k] = to_icrs.apply(along).map(|u| u * mpc * per_mpc);
    }
    Ok(Some(Extents {
        frame,
        axes,
        from: names.names().map(pointers::value).to_vec(),
        method: EXTENTS_METHOD.to_owned(),
    }))
}

fn pointed(from: &[Pointed<'_>]) -> Vec<String> {
    from.iter().map(|(p, _)| p.clone()).collect()
}

/// A published position as a point, with its derivation.
fn point(
    s: &Structure,
    at: &Coordinates,
    p: &Placing<'_>,
) -> Result<(Geometry, Derivation), String> {
    let (centre, _) = place(at, p.release, p.correction)?;
    let derived = derivation(
        pointed(&pointers::position(s)),
        frame_chain(at.frame()),
        coordinates_distance(at, &p.dipole),
    );
    Ok((Geometry::Point { at: centre }, derived))
}

/// A member list placed whole, with its derivation.
fn members(
    m: &Members,
    placed: &MemberPlacement,
    shape: &Shape,
    dipole: &str,
) -> (Geometry, Derivation) {
    let mut from = vec!["/geometry/Members".to_owned()];
    for key in crate::crossmatch::STRUCTURE_REDSHIFTS {
        let named = format!("the structure's value {key}");
        if placed.from_file.iter().any(|f| f.redshift_from == named) {
            from.push(pointers::value(key));
        }
    }
    let derived = derivation(
        from,
        frame_chain(Some(m.frame)),
        distance(
            "each member's matched object, or, for a member no object carries, its own sky \
             position and redshift in the structure file",
            format!(
                "a matched member at its object's derived position; any other member a tracer at \
                 the comoving distance of its redshift, taken as heliocentric and moved to the CMB \
                 frame with the {dipole} dipole toward it, under the release cosmology"
            ),
        ),
    );
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
        Geometry::GridRegion {
            grid,
            origin,
            cell,
            shape,
            cells,
        } => Geometry::GridRegion {
            grid,
            origin: s(origin),
            cell: cell * per_mpc,
            shape,
            cells,
        },
        other => other,
    }
}

/// `geometry`, placed in Mpc in `frame`, as a shape in the unit of `domain`
/// (`crate::tiles::unit_of`).
///
/// # Errors
///
/// Fails when the shape cannot stand.
pub(super) fn in_domain(
    frame: Frame,
    geometry: Geometry,
    domain: ScaleDomain,
) -> Result<Shape, String> {
    let unit = unit_of(domain);
    let per_mpc = Unit::Megaparsec
        .factor_to(unit)
        .map_err(|e| e.to_string())?;
    Shape::new(frame, unit, in_units(geometry, per_mpc))
        .map_err(|e| format!("its shape cannot stand: {e}"))
}

/// The shape of `s` as placed under `p` from the geometry its file gives, its member list
/// `placed` where it has one, and its derivation; or why it cannot be placed, as
/// [`super::index_bounds`] says.
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
        Some(g @ StructureGeometry::Primitive(sphere)) => {
            let (centre, _) = place(&sphere.centre, p.release, p.correction)?;
            let radius = sphere_radius(sphere, p.release)?;
            let geometry = Geometry::Primitive {
                primitive: Primitive::Sphere { centre, radius },
            };
            let derived = derivation(
                pointed(&pointers::geometry(g)),
                frame_chain(sphere.centre.frame()),
                coordinates_distance(&sphere.centre, &p.dipole),
            );
            (geometry, derived)
        }
        Some(g @ StructureGeometry::SkyRegion(b)) => {
            let (near, far) = (radial(&b.near, p.release)?, radial(&b.far, p.release)?);
            let geometry = Geometry::SkyRegion {
                order: REGION_ORDER,
                cells: box_cells(b, REGION_ORDER)?,
                near: near.min(far),
                far: near.max(far),
            };
            let bound = radial_distance(&b.near, None, &p.dipole);
            let mut derived = derivation(
                pointed(&pointers::geometry(g)),
                frame_chain(Some(b.frame)),
                distance(
                    format!("the published near and far bounds, each {}", bound.from),
                    format!(
                        "{}, for each bound; the cells of HEALPix order {REGION_ORDER} whose \
                         centres fall in {}",
                        bound.method,
                        b.sides.described()
                    ),
                ),
            );
            derived.at_least = b
                .at_least
                .as_ref()
                .map(|_| "/geometry/SkyRegion/at_least".to_owned());
            (geometry, derived)
        }
        None => point(s, s.position.as_ref().ok_or_else(no_position)?, p)?,
    };
    let shape = in_domain(Frame::Icrs, geometry, s.scale_domain)?;
    let mut derivation = derivation;
    if matches!(shape.geometry, Geometry::Point { .. }) {
        derivation.extents = extents(s, p.release, shape.unit)?;
    }
    Ok(PlacedShape { shape, derivation })
}
