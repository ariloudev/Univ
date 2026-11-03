//! The tile rules of a release, read from `rules/tiles.yaml`: when a node splits, how many
//! points the fullest inner node of a level keeps, how deep the octree goes and the seed of the
//! subsample.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::tile::{MAX_POINTS, tile_bytes};

use crate::yaml::{FileError, read_checked};

/// The deepest octree the rules may ask for: a node's address then has 31 characters, and its
/// edge is 2^-30 of the root's.
pub const DEEPEST: u8 = 30;

/// The largest seed: a manifest's numbers reach TypeScript clients, which hold integers exactly
/// up to 2^53 − 1.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// The tile rules and their bases.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileRules {
    pub statement: String,
    /// A node holding more points than this is split, above the maximum depth.
    pub split_threshold: usize,
    pub split_basis: String,
    /// The most points an inner node keeps: the fullest inner node of a level keeps this many,
    /// and the others the same fraction of theirs.
    pub level_points: usize,
    pub level_points_basis: String,
    /// The deepest level; a node there is never split.
    pub max_depth: u8,
    pub max_depth_basis: String,
    /// The seed of the subsample, recorded in each tile manifest.
    pub seed: u64,
    pub seed_basis: String,
}

impl TileRules {
    /// Reads and checks the rules in `file`.
    ///
    /// # Errors
    ///
    /// Fails naming the field when the file does not parse or a parameter cannot stand.
    pub fn read(file: &Path) -> Result<Self, FileError> {
        read_checked(file, Self::check)
    }

    /// The checks serde cannot make: every tile the rules allow above the maximum depth must fit
    /// within 1 MiB.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem.
    pub fn check(&self) -> Result<(), (String, String)> {
        let bad = |f: &str, p: String| Err((f.to_owned(), p));
        let fits = |n: usize| {
            format!(
                "must lie in 1 to {MAX_POINTS}, the most points a tile holds within 1 MiB ({n} \
                 points take {} bytes)",
                tile_bytes(n)
            )
        };
        if !(1..=MAX_POINTS).contains(&self.split_threshold) {
            return bad("split_threshold", fits(self.split_threshold));
        }
        if !(1..=self.split_threshold).contains(&self.level_points) {
            return bad(
                "level_points",
                format!(
                    "must lie in 1 to the split threshold, {}",
                    self.split_threshold
                ),
            );
        }
        if self.max_depth > DEEPEST {
            return bad("max_depth", format!("must be at most {DEEPEST}"));
        }
        if self.seed > MAX_SEED {
            return bad("seed", "must be at most 2^53 - 1".to_owned());
        }
        Ok(())
    }
}
