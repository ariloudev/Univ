//! The box a researched structure enters the cell index with: its geometry placed under the
//! release cosmology, in the ICRS about the barycentre, in Mpc.
//!
//! - **Member lists** are bounded by their members' positions once every member is placed
//!   (`crate::crossmatch`). One whose members are not all placed is shown at its published
//!   position, and indexed there when that position can be placed.
//! - **Points**, a structure published with a position alone, are a box of no size.
//! - **Spheres** are the box about their placed centre, its half-side the radius.
//! - **Sky regions** are bounded by the region itself, the directions of their longitude and
//!   latitude ranges between the near and far distances, rather than by the sphere of the far
//!   distance; [`sky_region`] gives how.
//!
//! # Placing a published position
//!
//! A redshift, or a recession velocity cz taken as the redshift v / c, becomes the comoving
//! distance of the release cosmology. A length published for another cosmology (`assumes`, or
//! h⁻¹ Mpc for H0 = 100) is rescaled by the ratio of Hubble constants
//! (`univ_model::derive::Release::hubble_rescaling`); a length that states none is taken as
//! published. Cartesian velocities, as the Cosmicflows papers give them, are a direction and a
//! velocity whose comoving distance is taken the same way. The frame is turned into the ICRS,
//! FK4 B1950 with its E-terms. Equatorial coordinates of an unstated equinox are placed as J2000,
//! and the box grows on every side by the distance times the sine of the B1950 to J2000 angle,
//! so the box holds the structure under either reading.
//!
//! The box is a candidate filter, so each approximation errs large: it is never smaller than
//! the published geometry so placed.

use univ_model::cosmology::{AssumedCosmology, SPEED_OF_LIGHT};
use univ_model::derive::Release;
use univ_model::geometry::Aabb;
use univ_model::position::{Frame, Vec3, between, dot, lon_lat, scaled, unit_vector};
use univ_model::units::{Dimension, Unit};

use crate::crossmatch::MemberPlacement;
use crate::structures::{
    Cited, Coordinates, Radial, SkyBox, SphereSize, Structure, StructureGeometry,
};

/// The samples along each side of a sky region's longitude and latitude ranges.
const SKY_SAMPLES: u32 = 32;

/// The box of `structure` in the cell index, under `release`, its member list `placed` where
/// it has one; or why it has none.
///
/// # Errors
///
/// Fails, saying why, for a structure that publishes no position or geometry, one whose member
/// list is not placed and whose published position cannot be, or one whose distances cannot be
/// placed.
pub fn index_bounds(
    structure: &Structure,
    placed: Option<&MemberPlacement>,
    release: &Release,
) -> Result<Aabb, String> {
    match &structure.geometry {
        Some(StructureGeometry::Members(_)) => match placed {
            Some(p) if p.shape.is_some() => p
                .bounds
                .ok_or_else(|| "its member list has no bounding box".to_owned()),
            Some(p) => {
                let unplaced = format!(
                    "{} of its {} members are not placed",
                    p.unplaced.len(),
                    p.members
                );
                match &structure.position {
                    Some(at) => point(at, release)
                        .map_err(|e| format!("{unplaced}, and its published position: {e}")),
                    None => Err(unplaced),
                }
            }
            None => Err("its member list was not resolved".to_owned()),
        },
        Some(StructureGeometry::Primitive(p)) => {
            let (centre, margin) = place(&p.centre, release)?;
            let radius = match &p.size {
                SphereSize::Radius(r) => length(r, release)?,
                SphereSize::Diameter(d) => length(d, release)? / 2.0,
            };
            Ok(centred(centre, radius + margin))
        }
        Some(StructureGeometry::SkyRegion(b)) => sky_region(b, release),
        None => match &structure.position {
            Some(at) => point(at, release),
            None => Err("it publishes no position or geometry".to_owned()),
        },
    }
}

/// The centre of `structure`'s geometry in the ICRS about the barycentre, in Mpc under
/// `release`, its member list `placed` where it has one: its published position for a point or
/// a member list not all placed, a sphere's centre, the middle of a sky region (its middle
/// longitude and latitude, halfway between its near and far distances), and the centre of the
/// box of a member list's placed members; or why it has none.
///
/// # Errors
///
/// As [`index_bounds`].
pub fn centre(
    structure: &Structure,
    placed: Option<&MemberPlacement>,
    release: &Release,
) -> Result<Vec3, String> {
    let middle = |b: Aabb| -> Vec3 { std::array::from_fn(|k| f64::midpoint(b.min[k], b.max[k])) };
    match &structure.geometry {
        Some(StructureGeometry::Members(_)) => index_bounds(structure, placed, release).map(middle),
        Some(StructureGeometry::Primitive(p)) => place(&p.centre, release).map(|(c, _)| c),
        Some(StructureGeometry::SkyRegion(b)) => {
            let near = radial(&b.near, release)?;
            let far = radial(&b.far, release)?;
            let lon0 = degrees(&b.lon[0])?;
            let lon = lon0 + longitude_span(lon0, degrees(&b.lon[1])?) / 2.0;
            let lat = f64::midpoint(degrees(&b.lat[0])?, degrees(&b.lat[1])?);
            let u = unit_vector(lon, lat);
            Ok(between(b.frame, Frame::Icrs).apply(scaled(u, f64::midpoint(near, far))))
        }
        None => match &structure.position {
            Some(at) => place(at, release).map(|(c, _)| c),
            None => Err("it publishes no position or geometry".to_owned()),
        },
    }
}

/// The span of a longitude range from `lon0` to `lon1`, degrees: a full turn or more, such as
/// [0, 360], spans every longitude, not none.
fn longitude_span(lon0: f64, lon1: f64) -> f64 {
    match lon1 - lon0 {
        whole if whole >= 360.0 => 360.0,
        part => part.rem_euclid(360.0),
    }
}

fn centred(centre: Vec3, half: f64) -> Aabb {
    Aabb {
        min: centre.map(|c| c - half),
        max: centre.map(|c| c + half),
    }
}

/// The box of a published position: of no size, or the size of the unstated equinox's angle.
fn point(at: &Coordinates, release: &Release) -> Result<Aabb, String> {
    let (p, margin) = place(at, release)?;
    Ok(centred(p, margin))
}

/// The comoving distance of redshift `z` in the release cosmology, Mpc.
fn comoving(z: f64, release: &Release) -> Result<f64, String> {
    release
        .cosmology()
        .comoving_distance(z)
        .map_err(|e| e.to_string())
}

fn km_s(c: &Cited) -> Result<f64, String> {
    c.quantity
        .to(Unit::KilometrePerSecond)
        .map(|q| q.value)
        .map_err(|e| e.to_string())
}

fn degrees(c: &Cited) -> Result<f64, String> {
    c.quantity
        .to(Unit::Degree)
        .map(|q| q.value)
        .map_err(|e| e.to_string())
}

/// A published length in Mpc of the release cosmology.
fn length(c: &Cited, release: &Release) -> Result<f64, String> {
    let assumed = if c.unit().dimension() == Dimension::HubbleScaledLength {
        Some(AssumedCosmology::HUBBLE_SCALED)
    } else {
        c.assumes.as_ref().and_then(|a| a.cosmology)
    };
    let quantity = match assumed {
        Some(a) => {
            release
                .hubble_rescaling(a)
                .and_then(|r| r.apply(&c.quantity))
                .map_err(|e| e.to_string())?
                .rescaled
        }
        None => c.quantity,
    };
    quantity
        .to(Unit::Megaparsec)
        .map(|q| q.value)
        .map_err(|e| e.to_string())
}

/// A published distance along the line of sight in Mpc of the release cosmology.
fn radial(r: &Radial, release: &Release) -> Result<f64, String> {
    match r {
        Radial::Redshift(z) => comoving(z.value(), release),
        Radial::Velocity(v) => comoving(km_s(v)? / SPEED_OF_LIGHT, release),
        Radial::Distance(d) => length(d, release),
    }
}

/// How much a box at `distance` grows for the frame's systematic angle.
fn margin(frame: Frame, distance: f64) -> f64 {
    frame
        .systematic_uncertainty()
        .map_or(0.0, |deg| distance * deg.to_radians().sin())
}

/// A published position in the ICRS and Mpc, with how much its box grows for its frame.
fn place(at: &Coordinates, release: &Release) -> Result<(Vec3, f64), String> {
    match at {
        Coordinates::Observer => Ok(([0.0; 3], 0.0)),
        Coordinates::Cartesian { frame, x, y, z } => {
            let v = if x.unit().dimension() == Dimension::Velocity {
                let v = [km_s(x)?, km_s(y)?, km_s(z)?];
                let speed = dot(v, v).sqrt();
                if speed == 0.0 {
                    v
                } else {
                    scaled(v, comoving(speed / SPEED_OF_LIGHT, release)? / speed)
                }
            } else {
                [
                    length(x, release)?,
                    length(y, release)?,
                    length(z, release)?,
                ]
            };
            let p = between(*frame, Frame::Icrs).apply(v);
            Ok((p, margin(*frame, dot(p, p).sqrt())))
        }
        Coordinates::Spherical {
            frame,
            lon,
            lat,
            radial: r,
        } => {
            let r = r
                .as_ref()
                .ok_or_else(|| "its published position gives no distance".to_owned())?;
            let d = radial(r, release)?;
            let lon = lon.as_ref().map_or(Ok(0.0), degrees)?;
            let u = unit_vector(lon, degrees(lat)?);
            Ok((
                between(*frame, Frame::Icrs).apply(scaled(u, d)),
                margin(*frame, d),
            ))
        }
    }
}

/// The box of a sky region between its near and far distances.
///
/// Its directions are sampled on a grid of [`SKY_SAMPLES`] steps along each of its longitude
/// and latitude ranges, edges included, and turned into the ICRS; the range of each ICRS
/// component of those directions, widened to ±1 where the region holds that axis, times the
/// near and far distances, bounds the region. Between samples a component changes by at most
/// the angle between them, so the box grows by the far distance times that angle (and by the
/// unstated equinox's angle, where there is one).
///
/// # Errors
///
/// Fails when the near or far distance cannot be placed.
pub fn sky_region(region: &SkyBox, release: &Release) -> Result<Aabb, String> {
    let near = radial(&region.near, release)?;
    let far = radial(&region.far, release)?;
    let (near, far) = (near.min(far), near.max(far));
    let lon0 = degrees(&region.lon[0])?;
    let span = longitude_span(lon0, degrees(&region.lon[1])?);
    let (lat_a, lat_b) = (degrees(&region.lat[0])?, degrees(&region.lat[1])?);
    let (lat0, lat1) = (lat_a.min(lat_b), lat_a.max(lat_b));
    let to_icrs = between(region.frame, Frame::Icrs);
    let n = f64::from(SKY_SAMPLES);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for i in 0..=SKY_SAMPLES {
        for j in 0..=SKY_SAMPLES {
            let lon = lon0 + span * f64::from(i) / n;
            let lat = lat0 + (lat1 - lat0) * f64::from(j) / n;
            let u = to_icrs.apply(unit_vector(lon, lat));
            for k in 0..3 {
                lo[k] = lo[k].min(u[k]);
                hi[k] = hi[k].max(u[k]);
            }
        }
    }
    // An ICRS axis inside the region is its extreme along that axis.
    let from_icrs = between(Frame::Icrs, region.frame);
    for k in 0..3 {
        for sign in [1.0, -1.0] {
            let mut axis = [0.0; 3];
            axis[k] = sign;
            let (lon, lat) = lon_lat(from_icrs.apply(axis));
            let inside_lon = (lon - lon0).rem_euclid(360.0) <= span;
            if inside_lon && (lat0..=lat1).contains(&lat) {
                lo[k] = lo[k].min(sign);
                hi[k] = hi[k].max(sign);
            }
        }
    }
    let step = (span.max(lat1 - lat0) / n).to_radians();
    let pad = far * step + margin(region.frame, far);
    let along = |c: f64| [near * c, far * c];
    let min: Vec3 =
        std::array::from_fn(|k| along(lo[k]).into_iter().fold(f64::INFINITY, f64::min) - pad);
    let max: Vec3 =
        std::array::from_fn(|k| along(hi[k]).into_iter().fold(f64::NEG_INFINITY, f64::max) + pad);
    Ok(Aabb { min, max })
}
