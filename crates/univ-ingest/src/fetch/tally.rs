//! What a fetch came to: the items it fetched, those already on disk, and the bytes it fetched.

use std::sync::atomic::{AtomicU64, Ordering};

/// What a fetch came to so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Chunks and files fetched whole.
    pub got: u64,
    /// Chunks and files already on disk.
    pub hit: u64,
    /// Bytes fetched, whether or not their item was finished; not what was on disk before.
    pub bytes: u64,
}

/// The counts behind a [`Tally`], added to as the fetch goes.
#[derive(Debug, Default)]
pub(super) struct Counter {
    got: AtomicU64,
    hit: AtomicU64,
    bytes: AtomicU64,
}

impl Counter {
    pub(super) fn got(&self) {
        self.got.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn hit(&self) {
        self.hit.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn bytes(&self, n: u64) {
        self.bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub(super) fn tally(&self) -> Tally {
        Tally {
            got: self.got.load(Ordering::Relaxed),
            hit: self.hit.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}
