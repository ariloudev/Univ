//! A 128-bit hash, for telling apart millions of keys or identifiers without keeping them.

use std::hash::{DefaultHasher, Hash, Hasher};

/// A 128-bit hash of what `feed` writes: two `SipHash` runs of it, the second after a seed, so
/// that the halves differ. Two of n values share a hash with a chance of about n² / 2^129,
/// 10^-24 for twenty million.
pub fn hash128(feed: impl Fn(&mut DefaultHasher)) -> u128 {
    let mut a = DefaultHasher::new();
    let mut b = DefaultHasher::new();
    0x5eed_u32.hash(&mut b);
    feed(&mut a);
    feed(&mut b);
    (u128::from(a.finish()) << 64) | u128::from(b.finish())
}
