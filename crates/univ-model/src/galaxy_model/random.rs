//! The Galaxy model's random numbers: `ChaCha20`, keyed by the model's seed as the tiles' own
//! subsample is (the seed's 8 little-endian bytes, then 24 zero bytes), each population on its
//! own stream, its code, so that a population's points do not depend on any other's.

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::Rng;

use crate::seeded::generator;

/// A seeded source of the draws the model's sampling makes.
pub struct Random {
    rng: ChaCha20Rng,
    /// The second of a pair of normal draws, kept for the next call.
    spare: Option<f64>,
}

impl Random {
    /// The draws of stream `stream` under `seed`.
    #[must_use]
    pub fn new(seed: u64, stream: u64) -> Self {
        Self {
            rng: generator(seed, stream),
            spare: None,
        }
    }

    /// A number in [0, 1), from the top 53 bits of a 64-bit draw.
    #[expect(clippy::cast_precision_loss, reason = "53 bits, exact in a double")]
    pub fn uniform(&mut self) -> f64 {
        (self.rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number in (0, 1], for a logarithm.
    pub fn open(&mut self) -> f64 {
        1.0 - self.uniform()
    }

    /// A standard normal number, by the Box–Muller transform.
    pub fn normal(&mut self) -> f64 {
        if let Some(z) = self.spare.take() {
            return z;
        }
        let (u, v) = (self.open(), self.uniform());
        let r = (-2.0 * u.ln()).sqrt();
        let theta = std::f64::consts::TAU * v;
        self.spare = Some(r * theta.sin());
        r * theta.cos()
    }

    /// An exponential number of scale `scale`, positive.
    pub fn exponential(&mut self, scale: f64) -> f64 {
        -scale * self.open().ln()
    }

    /// A Laplace number of scale `scale`: an exponential of either sign, as a height above or
    /// below an exponential disc's plane.
    pub fn laplace(&mut self, scale: f64) -> f64 {
        let e = self.exponential(scale);
        if self.uniform() < 0.5 { -e } else { e }
    }
}
