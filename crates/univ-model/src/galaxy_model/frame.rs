//! From the model's Galactocentric frame to the tiles' ICRS Cartesian parsecs about the Solar
//! System barycentre, and from a heliocentric galactic path to the model's frame.

use crate::position::{Frame, Transform, Vec3, between};

/// Where the model's Galaxy stands from the Sun.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// The Galactic Centre's distance from the Sun, kpc, toward l = 0°, b = 0°.
    r0: f64,
    /// The Sun's height above the Galactic plane, kpc.
    sun_height: f64,
    to_icrs: Transform,
}

impl Placement {
    #[must_use]
    pub fn new(r0_kpc: f64, sun_height_kpc: f64) -> Self {
        Self {
            r0: r0_kpc,
            sun_height: sun_height_kpc,
            to_icrs: between(Frame::Galactic, Frame::Icrs),
        }
    }

    /// The point `p`, Galactocentric kpc (x from the Galactic Centre away from the Sun, y toward
    /// l = 90° at the Sun, z toward the north Galactic pole from the plane), in ICRS Cartesian
    /// parsecs about the barycentre: heliocentric galactic first, the Sun `r0` from the Centre
    /// toward l = 180° and `sun_height` above the plane, then turned by the Hipparcos rotation.
    #[must_use]
    pub fn to_icrs_pc(&self, p: [f64; 3]) -> Vec3 {
        let galactic = [p[0] + self.r0, p[1], p[2] - self.sun_height];
        self.to_icrs.apply(galactic).map(|c| c * 1000.0)
    }

    /// The point `p` of a heliocentric galactic path, kpc, in the plane of the model's frame.
    #[must_use]
    pub fn in_plane(&self, p: [f64; 3]) -> [f64; 2] {
        [p[0] - self.r0, p[1]]
    }
}
