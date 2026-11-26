//! The seeded generator every random draw of a release is made with: `ChaCha20`, keyed by the
//! seed's eight little-endian bytes and 24 zero bytes, on a stream of its own, so that the same
//! seed and stream give the same draws (the tiles' subsample and the Galaxy model's sampling).

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;

/// The `ChaCha20` generator of `stream` under `seed`.
#[must_use]
pub fn generator(seed: u64, stream: u64) -> ChaCha20Rng {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());
    let mut rng = ChaCha20Rng::from_seed(key);
    rng.set_stream(stream);
    rng
}
