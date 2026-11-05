//! A sky index: declination bands of the search radius' height, each cut into right-ascension
//! cells at least as wide as the radius at the band's poleward edge. A query reads the cells
//! that the cap of the radius around a position can touch, so it finds every point within the
//! radius. Built once by sorting, it holds one cell key and one index per point.

/// Points by cell, for finding the neighbours of a position within a fixed radius.
pub struct SkyIndex {
    /// Band height and search radius, degrees.
    radius: f64,
    /// Cells per band, by band.
    cells: Vec<u32>,
    /// (cell key, point), sorted by key.
    points: Vec<(u64, u32)>,
}

impl SkyIndex {
    /// An index of `positions`, (right ascension, declination) in degrees, by their place in
    /// the slice, for searches of `radius` degrees.
    #[must_use]
    pub fn new(positions: &[(f64, f64)], radius: f64) -> Self {
        // A floor keeps the bands to a few million at most, should a radius be tiny.
        let radius = radius.max(1e-4);
        let bands = band_of(90.0, radius) + 1;
        let cells = (0..bands)
            .map(|b| {
                let low = -90.0 + f64::from(b) * radius;
                let edge = low.abs().max((low + radius).abs()).min(90.0);
                let width = radius / edge.to_radians().cos();
                if width.is_finite() && width < 360.0 {
                    count(360.0 / width)
                } else {
                    1
                }
            })
            .collect();
        let mut index = Self {
            radius,
            cells,
            points: Vec::with_capacity(positions.len()),
        };
        for (i, &(ra, dec)) in positions.iter().enumerate() {
            let band = band_of(dec, radius);
            let cell = index.cell_of(band, ra);
            index.points.push((cell_key(band, cell), point(i)));
        }
        index.points.sort_unstable();
        index
    }

    fn cell_of(&self, band: u32, ra: f64) -> u32 {
        let n = self.cells[band as usize];
        count(ra.rem_euclid(360.0) / 360.0 * f64::from(n)).min(n - 1)
    }

    /// Every point that may lie within the radius of (`ra`, `dec`), and some beyond it: the
    /// caller measures each.
    pub fn near(&self, ra: f64, dec: f64, out: &mut Vec<u32>) {
        out.clear();
        let r = self.radius;
        let first = band_of((dec - r).max(-90.0), r);
        let last = band_of((dec + r).min(90.0), r);
        // The half-width in right ascension of a cap of radius r centred at dec.
        let half = if dec.abs() + r >= 90.0 {
            None
        } else {
            let s = r.to_radians().sin() / dec.to_radians().cos();
            Some(s.min(1.0).asin().to_degrees())
        };
        for band in first..=last {
            let n = self.cells[band as usize];
            match half {
                Some(h) if 2.0 * h < 360.0 - 360.0 / f64::from(n) => {
                    let from = self.cell_of(band, ra - h);
                    let to = self.cell_of(band, ra + h);
                    let mut c = from;
                    loop {
                        self.read(cell_key(band, c), out);
                        if c == to {
                            break;
                        }
                        c = (c + 1) % n;
                    }
                }
                _ => {
                    let (lo, hi) = (cell_key(band, 0), cell_key(band, n));
                    let start = self.points.partition_point(|(k, _)| *k < lo);
                    let end = self.points.partition_point(|(k, _)| *k < hi);
                    out.extend(self.points[start..end].iter().map(|(_, i)| *i));
                }
            }
        }
    }

    fn read(&self, key: u64, out: &mut Vec<u32>) {
        let start = self.points.partition_point(|(k, _)| *k < key);
        out.extend(
            self.points[start..]
                .iter()
                .take_while(|(k, _)| *k == key)
                .map(|(_, i)| *i),
        );
    }
}

fn band_of(dec: f64, radius: f64) -> u32 {
    count((dec + 90.0) / radius)
}

fn cell_key(band: u32, cell: u32) -> u64 {
    (u64::from(band) << 32) | u64::from(cell)
}

/// A non-negative finite count, floored.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is non-negative and bounded by 360 / 1e-4 cells or 180 / 1e-4 bands"
)]
fn count(x: f64) -> u32 {
    x.max(0.0).floor() as u32
}

/// A point's place, kept as u32 to halve the index: 4 billion points is beyond any release.
fn point(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use univ_model::position::separation_degrees;
    use univ_model::position::unit_vector;

    /// A simulated sky of points spread by a fixed generator: the index finds every pair
    /// within the radius that a full search finds.
    #[test]
    fn the_index_finds_every_neighbour_a_full_search_finds() {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            #[expect(clippy::cast_precision_loss, reason = "a uniform draw")]
            let u = (state >> 11) as f64 / (1u64 << 53) as f64;
            u
        };
        let mut sky: Vec<(f64, f64)> = (0..3_000)
            .map(|_| (next() * 360.0, (next() * 2.0 - 1.0).asin().to_degrees()))
            .collect();
        // Points at the poles and across RA = 0.
        sky.extend([
            (10.0, 89.99),
            (190.0, 89.995),
            (359.999, 0.0),
            (0.0005, 0.0),
        ]);
        let radius = 3.0;
        let index = SkyIndex::new(&sky, radius);
        let mut out = Vec::new();
        for (i, &(ra, dec)) in sky.iter().enumerate() {
            index.near(ra, dec, &mut out);
            let a = unit_vector(ra, dec);
            for (j, &(rb, db)) in sky.iter().enumerate() {
                if separation_degrees(a, unit_vector(rb, db)) <= radius {
                    assert!(out.contains(&point(j)), "{i} misses {j}");
                }
            }
        }
    }
}
