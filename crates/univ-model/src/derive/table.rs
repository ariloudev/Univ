//! The comoving distance of a cosmology tabulated once at log-spaced redshifts, read by cubic
//! Hermite interpolation with the exact slope `c / H(z)` at each node, and inverted by the same
//! interpolation with the roles swapped and one Newton step.

use std::sync::Arc;

use crate::cosmology::{Cosmology, CosmologyError, MAX_REDSHIFT};

/// The lowest redshift node after z = 0. Below it the first segment, from 0, interpolates.
const LOWEST: f64 = 1e-5;
/// Nodes from [`LOWEST`] to [`MAX_REDSHIFT`], log-spaced: a step of 2.3 % in z, which leaves
/// the interpolation within 1e-9 of the integral.
const NODES: usize = 600;
/// Simpson intervals across each segment between nodes.
const SEGMENT_INTERVALS: u32 = 16;

/// `D_C(z)` of one cosmology at [`NODES`] + 1 redshifts, with its slope at each.
#[derive(Clone, Debug, PartialEq)]
pub struct ComovingTable {
    /// Shared with every distance and position derived under it: a clone of its name for each
    /// of millions of objects would be a heap string each.
    cosmology: Arc<Cosmology>,
    redshift: Vec<f64>,
    distance: Vec<f64>,
    slope: Vec<f64>,
}

impl ComovingTable {
    /// Tabulates `cosmology`, integrating 1 / E(z) segment by segment with Simpson's rule.
    #[must_use]
    pub fn new(cosmology: &Cosmology) -> Self {
        let ratio = (MAX_REDSHIFT / LOWEST).ln();
        #[expect(
            clippy::cast_precision_loss,
            reason = "node indices are far below 2^52"
        )]
        let node = |i: usize| LOWEST * (ratio * (i - 1) as f64 / (NODES - 1) as f64).exp();
        let mut redshift: Vec<f64> = (0..=NODES)
            .map(|i| if i == 0 { 0.0 } else { node(i) })
            .collect();
        redshift[NODES] = MAX_REDSHIFT;
        let hubble = cosmology.hubble_distance();
        let slope: Vec<f64> = redshift
            .iter()
            .map(|&z| hubble / cosmology.expansion(z))
            .collect();
        let mut distance = Vec::with_capacity(redshift.len());
        distance.push(0.0);
        for pair in redshift.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let h = (b - a) / f64::from(SEGMENT_INTERVALS);
            let f = |k: u32| 1.0 / cosmology.expansion(a + h * f64::from(k));
            let inner: f64 = (1..SEGMENT_INTERVALS)
                .map(|k| if k % 2 == 1 { 4.0 * f(k) } else { 2.0 * f(k) })
                .sum();
            let segment = hubble * h / 3.0 * (f(0) + inner + f(SEGMENT_INTERVALS));
            let last = distance.last().copied().unwrap_or(0.0);
            distance.push(last + segment);
        }
        Self {
            cosmology: Arc::new(cosmology.clone()),
            redshift,
            distance,
            slope,
        }
    }

    /// The cosmology tabulated.
    #[must_use]
    pub fn cosmology(&self) -> &Cosmology {
        &self.cosmology
    }

    /// The cosmology tabulated, shared.
    #[must_use]
    pub fn shared_cosmology(&self) -> &Arc<Cosmology> {
        &self.cosmology
    }

    /// The comoving distance to redshift `z`, in Mpc.
    ///
    /// # Errors
    ///
    /// Fails on a negative or non-finite redshift, or one beyond [`MAX_REDSHIFT`].
    pub fn distance(&self, z: f64) -> Result<f64, CosmologyError> {
        if !(0.0..=MAX_REDSHIFT).contains(&z) {
            return Err(CosmologyError::Redshift(z));
        }
        let i = segment(&self.redshift, z);
        Ok(hermite(
            (self.redshift[i], self.redshift[i + 1]),
            (self.distance[i], self.distance[i + 1]),
            (self.slope[i], self.slope[i + 1]),
            z,
        ))
    }

    /// The slope `dD_C/dz = c / H(z)` in Mpc at redshift `z`, exact rather than interpolated.
    ///
    /// # Errors
    ///
    /// Fails on a negative or non-finite redshift, or one beyond [`MAX_REDSHIFT`].
    pub fn slope(&self, z: f64) -> Result<f64, CosmologyError> {
        if !(0.0..=MAX_REDSHIFT).contains(&z) {
            return Err(CosmologyError::Redshift(z));
        }
        Ok(self.cosmology.hubble_distance() / self.cosmology.expansion(z))
    }

    /// The redshift whose comoving distance is `distance` Mpc: the inverse interpolation, with
    /// slope `1 / D_C'(z)`, then one Newton step on the forward interpolation.
    ///
    /// # Errors
    ///
    /// Fails on a negative or non-finite distance, or one beyond that of [`MAX_REDSHIFT`].
    pub fn redshift(&self, distance: f64) -> Result<f64, CosmologyError> {
        let far = self.distance[NODES];
        if !(distance.is_finite() && (0.0..=far).contains(&distance)) {
            return Err(CosmologyError::Distance(distance));
        }
        let i = segment(&self.distance, distance);
        let z = hermite(
            (self.distance[i], self.distance[i + 1]),
            (self.redshift[i], self.redshift[i + 1]),
            (1.0 / self.slope[i], 1.0 / self.slope[i + 1]),
            distance,
        )
        .clamp(0.0, MAX_REDSHIFT);
        let slope = self.cosmology.hubble_distance() / self.cosmology.expansion(z);
        Ok((z - (self.distance(z)? - distance) / slope).clamp(0.0, MAX_REDSHIFT))
    }
}

/// The index `i` of the segment `[nodes[i], nodes[i + 1]]` that holds `x`, for ascending
/// `nodes` and `x` within them.
fn segment(nodes: &[f64], x: f64) -> usize {
    nodes
        .partition_point(|&n| n <= x)
        .saturating_sub(1)
        .min(nodes.len() - 2)
}

/// The cubic Hermite interpolant through `(x0, y0)` and `(x1, y1)` with slopes `m0` and `m1`.
fn hermite((x0, x1): (f64, f64), (y0, y1): (f64, f64), (m0, m1): (f64, f64), x: f64) -> f64 {
    let h = x1 - x0;
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0
        + (t3 - 2.0 * t2 + t) * h * m0
        + (-2.0 * t3 + 3.0 * t2) * y1
        + (t3 - t2) * h * m1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relative(actual: f64, expected: f64) -> f64 {
        ((actual - expected) / expected).abs()
    }

    #[test]
    fn the_table_matches_the_integral_to_1e_6_from_1e_4_to_z_10() {
        let planck = Cosmology::planck_2018();
        let table = ComovingTable::new(&planck);
        let mut worst: f64 = 0.0;
        for k in 0..=400 {
            let z = 1e-4 * (1e5_f64.ln() * f64::from(k) / 400.0).exp();
            let z = z.min(MAX_REDSHIFT);
            let error = relative(
                table.distance(z).unwrap(),
                planck.comoving_distance(z).unwrap(),
            );
            worst = worst.max(error);
        }
        assert!(worst < 1e-6, "worst relative error {worst}");
        assert_eq!(table.distance(0.0).unwrap(), 0.0);
    }

    #[test]
    fn the_inverse_round_trips_and_refuses_what_lies_outside() {
        let table = ComovingTable::new(&Cosmology::planck_2018());
        for z in [0.0, 3e-6, 1e-4, 0.023, 0.1, 0.5, 1.0, 2.0, 7.5, 10.0] {
            let back = table.redshift(table.distance(z).unwrap()).unwrap();
            assert!(
                (back - z).abs() <= 1e-10 * z.max(1e-3),
                "{z} came back as {back}"
            );
        }
        assert!(table.distance(-1e-3).is_err());
        assert!(table.distance(10.5).is_err());
        assert!(table.redshift(-1.0).is_err());
        assert!(table.redshift(f64::NAN).is_err());
        let far = table.distance(MAX_REDSHIFT).unwrap();
        assert!(table.redshift(far * 1.001).is_err());
    }
}
