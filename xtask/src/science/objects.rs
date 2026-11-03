//! The checks of single objects and structures: a placed distance against a cited one, a sky
//! position against a cited direction, a structure's centre against a cited velocity.

use rusqlite::{Connection, OptionalExtension, params};
use univ_model::position::{Frame, between, lon_lat, separation_degrees, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel};

use super::rules::{Expect, ObjectRef, PlacedBy, VelocityFrame};
use super::{Finding, Release, num};

/// An object as the checks read it.
pub struct Object {
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub placed: bool,
    pub distance: Option<f64>,
    pub minus: Option<f64>,
    pub plus: Option<f64>,
    /// The source and method term of its preferred measurement.
    pub source: Option<String>,
    pub method: Option<String>,
}

/// The id of the object `object` names in release `number`, if exactly one.
///
/// # Errors
///
/// Fails when no object or more than one carries the designation or name.
pub fn resolve(db: &Connection, number: u32, object: &ObjectRef) -> Result<i64, String> {
    let ids: Vec<i64> = match object {
        ObjectRef::Designation { source, identifier } => collect(
            db,
            "SELECT DISTINCT object_id FROM designations WHERE release = ?1 AND source = \
             source(?2) AND identifier = ?3",
            params![number, source, identifier],
        )?,
        ObjectRef::Name { name } => collect(
            db,
            "SELECT DISTINCT object_id FROM names WHERE release = ?1 AND key = ?2",
            params![number, univ_model::release::rows::name_key(name)],
        )?,
    };
    match ids.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("no object of the release carries {object}")),
        _ => Err(format!("{} objects carry {object}: {ids:?}", ids.len())),
    }
}

fn collect(db: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<Vec<i64>, String> {
    let mut q = db.prepare(sql).map_err(|e| e.to_string())?;
    let rows = q.query_map(p, |r| r.get(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Reads object `id` of release `number`.
///
/// # Errors
///
/// Fails when the query cannot run or no such object exists.
pub fn read(db: &Connection, number: u32, id: i64) -> Result<Object, String> {
    db.query_row(
        "SELECT o.id, o.ra, o.dec, o.placed, o.distance_mpc, o.distance_minus, o.distance_plus, \
         (SELECT s.id FROM entries e JOIN sources s ON s.release = e.release AND s.n = e.source \
          WHERE e.release = o.release AND e.object_id = o.id AND e.entry = o.preferred_entry), \
         (SELECT t.term FROM measurements m JOIN terms t ON t.release = m.release AND t.id = \
          m.method WHERE m.release = o.release AND m.object_id = o.id AND m.entry = \
          o.preferred_entry AND m.quantity = o.preferred_quantity AND m.n = o.preferred_n) \
         FROM objects o WHERE o.release = ?1 AND o.id = ?2",
        params![number, id],
        |r| {
            Ok(Object {
                ra: r.get(1)?,
                dec: r.get(2)?,
                placed: r.get::<_, i64>(3)? == 1,
                distance: r.get(4)?,
                minus: r.get(5)?,
                plus: r.get(6)?,
                source: r.get(7)?,
                method: r.get(8)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("no object {id} in release {number}"))
}

/// A placed distance against the expected one.
///
/// # Errors
///
/// Fails when the object cannot be found or read.
pub fn distance(
    release: &Release<'_>,
    object: &ObjectRef,
    placed_by: Option<&PlacedBy>,
    expect: &Expect,
) -> Result<Finding, String> {
    let id = resolve(release.db, release.number, object)?;
    let o = read(release.db, release.number, id)?;
    let h0 = release.manifest.cosmology.hubble_constant;
    let scale = expect.hubble_constant.map_or(1.0, |cited| cited / h0);
    let value = expect.value.unwrap_or_default() * scale;
    let own = match (o.minus, o.plus) {
        (Some(m), Some(p)) => f64::midpoint(m, p),
        _ => 0.0,
    };
    let (tolerance, mut expected) = if let Some(sigma) = expect.sigma {
        let sigma = sigma * scale;
        let n = expect.n_sigma.unwrap_or(2.0);
        let combined = sigma.hypot(own);
        (
            n * combined,
            format!(
                "{} ± {} Mpc, within {n}σ = {} Mpc (cited σ and the object's {} in quadrature)",
                num(value),
                num(sigma),
                num(n * combined),
                num(own)
            ),
        )
    } else if let Some(absolute) = expect.absolute {
        (absolute, format!("{} ± {} Mpc", num(value), num(absolute)))
    } else {
        let t = expect.relative.unwrap_or_default() * value;
        (t, format!("{} Mpc within {}", num(value), num(t)))
    };
    if let Some(cited) = expect.hubble_constant {
        expected = format!(
            "{expected}; cited {} ± {} Mpc at H0 = {cited}, × {cited}/{h0} to the release's H0",
            num(expect.value.unwrap_or_default()),
            num(expect.sigma.unwrap_or_default())
        );
    }
    if let Some(p) = placed_by {
        expected = format!("{expected}, placed by {} ({})", p.source, p.method);
    }
    let by = format!(
        "{} ({})",
        o.source.as_deref().unwrap_or("-"),
        o.method.as_deref().unwrap_or("-")
    );
    let Some(d) = o.distance.filter(|_| o.placed) else {
        return Ok(Finding::new(
            format!("object {id} is not placed"),
            expected,
            false,
        ));
    };
    let placed_right = placed_by.is_none_or(|p| {
        o.source.as_deref() == Some(p.source.as_str()) && o.method.as_deref() == Some(&p.method)
    });
    let error = match (o.minus, o.plus) {
        (Some(m), Some(p)) => format!("−{} +{}", num(m), num(p)),
        _ => "no uncertainty reported".to_owned(),
    };
    let found = format!(
        "object {id} at {} Mpc ({error}), Δ = {}, placed by {by}",
        num(d),
        num(d - value),
    );
    Ok(Finding::new(
        found,
        expected,
        (d - value).abs() <= tolerance && placed_right,
    ))
}

/// An object's sky position against the expected direction.
///
/// # Errors
///
/// Fails when the object cannot be found or read.
pub fn direction(
    release: &Release<'_>,
    object: &ObjectRef,
    expect: &Expect,
) -> Result<Finding, String> {
    let id = resolve(release.db, release.number, object)?;
    let o = read(release.db, release.number, id)?;
    let frame = expect.frame.unwrap_or(Frame::Icrs);
    let (lon, lat) = (
        expect.lon.unwrap_or_default(),
        expect.lat.unwrap_or_default(),
    );
    let within = expect.within_arcmin.unwrap_or_default();
    let expected = format!("({lon}°, {lat}°) {frame:?} within {within}′");
    let (Some(ra), Some(dec)) = (o.ra, o.dec) else {
        return Ok(Finding::new(
            format!("object {id} has no position"),
            expected,
            false,
        ));
    };
    let icrs = unit_vector(ra, dec);
    let want = between(frame, Frame::Icrs).apply(unit_vector(lon, lat));
    let arcmin = 60.0 * separation_degrees(icrs, want);
    let (l, b) = lon_lat(between(Frame::Icrs, frame).apply(icrs));
    let found = format!(
        "object {id} at ({l:.5}°, {b:.5}°) {frame:?}, ICRS ({ra:.5}°, {dec:.5}°), {}′ off",
        num(arcmin)
    );
    Ok(Finding::new(found, expected, arcmin <= within))
}

/// A structure's centre distance against the comoving distance, under the release cosmology,
/// of the redshift of `velocity_km_s` (z = v / c), corrected from the heliocentric frame to the
/// CMB frame with the Planck 2018 dipole as the release corrects redshifts.
///
/// # Errors
///
/// Fails when the structure is missing or unplaced, or the redshift is beyond the model.
pub fn structure(
    release: &Release<'_>,
    id: &str,
    velocity_km_s: f64,
    frame: VelocityFrame,
    expect: &Expect,
) -> Result<Finding, String> {
    let centre: Option<[Option<f64>; 3]> = release
        .db
        .query_row(
            "SELECT centre_x, centre_y, centre_z FROM structures WHERE release = ?1 AND id = ?2",
            params![release.number, id],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?]),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let centre = centre.ok_or_else(|| format!("no structure {id} in the release"))?;
    let [Some(x), Some(y), Some(z)] = centre else {
        return Err(format!("structure {id} has no centre"));
    };
    let d = (x * x + y * y + z * z).sqrt();
    let toward = [x / d, y / d, z / d];
    let mut redshift = velocity_km_s / univ_model::cosmology::SPEED_OF_LIGHT;
    if frame == VelocityFrame::Heliocentric {
        let correction = CmbCorrection::new(DipoleModel::Planck2018);
        let dipole = DipoleModel::Planck2018.dipole();
        let beta = dipole.speed / univ_model::cosmology::SPEED_OF_LIGHT;
        let cos = univ_model::position::dot(toward, correction.apex_icrs());
        let factor = (1.0 + beta * cos) / (1.0 - beta * beta).sqrt();
        redshift = (1.0 + redshift) * factor - 1.0;
    }
    let want = release
        .manifest
        .cosmology
        .comoving_distance(redshift)
        .map_err(|e| e.to_string())?;
    let relative = expect.relative.unwrap_or_default();
    let expected = format!(
        "{} Mpc (z_cmb = {}) within {}%",
        num(want),
        num(redshift),
        num(relative * 100.0)
    );
    let found = format!(
        "centre at {} Mpc, {}% off",
        num(d),
        num(100.0 * (d / want - 1.0))
    );
    Ok(Finding::new(
        found,
        expected,
        (d - want).abs() <= relative * want,
    ))
}
