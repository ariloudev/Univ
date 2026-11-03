//! Pairs of placed objects close on the sky whose distances agree within their uncertainties:
//! the same galaxy entered twice. Objects are bucketed in declination bands one search radius
//! high and sorted by right ascension within each, so each object is compared only with those
//! in its own band and the next whose right ascension lies within the radius.

use std::collections::{BTreeMap, HashSet};

use rusqlite::Connection;
use univ_model::position::{separation_degrees, unit_vector};

/// A placed object as the search reads it.
#[derive(Clone, Debug)]
pub struct Placed {
    pub id: i64,
    pub ra: f64,
    pub dec: f64,
    pub distance: f64,
    /// The one-sigma distance uncertainty: the mean of its minus and plus, 0 when none.
    pub sigma: f64,
}

/// A pair found: the two objects, their separation in arcmin.
#[derive(Clone, Debug)]
pub struct Pair {
    pub a: Placed,
    pub b: Placed,
    pub arcmin: f64,
}

/// The placed objects of release `number` in `domains` (scale-domain terms) of `kinds`
/// (object-kind terms), through the `term` function the database has registered.
///
/// # Errors
///
/// Fails when the query cannot run, such as on a term the release does not hold.
pub fn placed(
    db: &Connection,
    number: u32,
    domains: &[String],
    kinds: &[String],
) -> Result<Vec<Placed>, String> {
    let list = |n: usize| vec!["term(?)"; n].join(", ");
    let sql = format!(
        "SELECT id, ra, dec, distance_mpc, distance_minus, distance_plus FROM objects \
         WHERE release = ? AND placed = 1 AND ra IS NOT NULL AND dec IS NOT NULL \
         AND distance_mpc IS NOT NULL AND scale_domain IN ({}) AND kind IN ({})",
        list(domains.len()),
        list(kinds.len())
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(number)];
    params.extend(
        domains
            .iter()
            .map(|d| Box::new(d.clone()) as Box<dyn rusqlite::ToSql>),
    );
    params.extend(
        kinds
            .iter()
            .map(|k| Box::new(k.clone()) as Box<dyn rusqlite::ToSql>),
    );
    let mut q = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = q
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let minus: Option<f64> = r.get(4)?;
            let plus: Option<f64> = r.get(5)?;
            Ok(Placed {
                id: r.get(0)?,
                ra: r.get(1)?,
                dec: r.get(2)?,
                distance: r.get(3)?,
                sigma: match (minus, plus) {
                    (Some(m), Some(p)) => f64::midpoint(m, p),
                    _ => 0.0,
                },
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Whether two distances agree within their uncertainties in quadrature.
fn agree(a: &Placed, b: &Placed) -> bool {
    (a.distance - b.distance).abs() <= a.sigma.hypot(b.sigma)
}

/// The indices, within `band` (sorted by right ascension), of the objects whose right
/// ascension lies within `width` degrees of `ra`, across 0h.
fn window(band: &[(f64, usize)], ra: f64, width: f64) -> Vec<usize> {
    if width >= 180.0 {
        return band.iter().map(|&(_, i)| i).collect();
    }
    let range = |lo: f64, hi: f64| {
        let start = band.partition_point(|&(r, _)| r < lo);
        let end = band.partition_point(|&(r, _)| r <= hi);
        band[start..end].iter().map(|&(_, i)| i)
    };
    let (lo, hi) = (ra - width, ra + width);
    let mut out: Vec<usize> = range(lo, hi).collect();
    if lo < 0.0 {
        out.extend(range(lo + 360.0, 360.0));
    }
    if hi >= 360.0 {
        out.extend(range(0.0, hi - 360.0));
    }
    out
}

/// Every pair of `objects` closer than `radius_arcmin` whose distances agree, each once, the
/// pairs with an object in `excepted` left out.
#[must_use]
pub fn close_pairs(objects: &[Placed], radius_arcmin: f64, excepted: &HashSet<i64>) -> Vec<Pair> {
    let radius = radius_arcmin / 60.0;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a band index of a declination within ±90° over a positive radius"
    )]
    let band_of = |dec: f64| ((dec + 90.0) / radius).floor() as i64;
    let mut bands: BTreeMap<i64, Vec<(f64, usize)>> = BTreeMap::new();
    for (i, o) in objects.iter().enumerate() {
        bands.entry(band_of(o.dec)).or_default().push((o.ra, i));
    }
    for band in bands.values_mut() {
        band.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let mut pairs = Vec::new();
    for (i, a) in objects.iter().enumerate() {
        let b0 = band_of(a.dec);
        // The widest right-ascension span the radius covers at the band edge nearer the pole.
        let edge = (a.dec.abs() + 2.0 * radius).min(90.0).to_radians().cos();
        let width = if edge <= radius.to_radians().sin() {
            180.0
        } else {
            radius / edge
        };
        let va = unit_vector(a.ra, a.dec);
        for b in [b0, b0 + 1] {
            let Some(band) = bands.get(&b) else { continue };
            for j in window(band, a.ra, width) {
                if (b == b0 && j <= i) || j == i {
                    continue;
                }
                let o = &objects[j];
                let arcmin = 60.0 * separation_degrees(va, unit_vector(o.ra, o.dec));
                if arcmin < radius_arcmin
                    && agree(a, o)
                    && !excepted.contains(&a.id)
                    && !excepted.contains(&o.id)
                {
                    pairs.push(Pair {
                        a: a.clone(),
                        b: o.clone(),
                        arcmin,
                    });
                }
            }
        }
    }
    pairs.sort_by(|x, y| x.a.distance.total_cmp(&y.a.distance));
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: i64, ra: f64, dec: f64, distance: f64, sigma: f64) -> Placed {
        Placed {
            id,
            ra,
            dec,
            distance,
            sigma,
        }
    }

    #[test]
    fn a_pair_is_found_once_across_bands_and_0h_and_needs_agreeing_distances() {
        let objects = [
            // Across 0h and a band edge, 0.6′ apart, distances within 1σ.
            at(1, 359.995, 10.0, 0.80, 0.03),
            at(2, 0.004, 10.0085, 0.82, 0.03),
            // Close on the sky but not at one distance.
            at(3, 120.0, -30.0, 0.80, 0.01),
            at(4, 120.0, -29.99, 2.0, 0.1),
            // At one distance but 2′ apart.
            at(5, 200.0, 0.0, 5.0, 0.5),
            at(6, 200.0, 0.0334, 5.0, 0.5),
            // Near the pole, where right ascension spans little.
            at(7, 10.0, 89.995, 1.0, 0.1),
            at(8, 190.0, 89.995, 1.0, 0.1),
        ];
        let pairs = close_pairs(&objects, 1.0, &HashSet::new());
        let ids: Vec<_> = pairs.iter().map(|p| (p.a.id, p.b.id)).collect();
        assert_eq!(ids, [(1, 2), (7, 8)]);
        assert!(pairs[0].arcmin < 1.0 && pairs[0].arcmin > 0.5);
        let none = close_pairs(&objects, 1.0, &HashSet::from([2, 8]));
        assert!(none.is_empty());
    }
}
