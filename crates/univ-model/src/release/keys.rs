//! Where a release's files lie in R2. A release keeps every file under its own prefix,
//! `releases/{n}/`, and a local build writes the same paths below its release directory, so a
//! file's key is its path with the prefix. Univ Server reads these keys; they stay stable.
//!
//! | Key | What it holds |
//! |---|---|
//! | `releases/{n}/manifest.yaml` | The release manifest: every other file with its SHA-256 |
//! | `releases/{n}/rules/{name}.yaml` | The matching, ranking, tile and release rules, verbatim |
//! | `releases/{n}/tiles/{domain}/manifest.json` | A scale domain's tile manifest |
//! | `releases/{n}/tiles/{domain}/{address}.bin` | One octree node's tile |
//! | `releases/{n}/grids/{grid}/header.json` | A volume grid's header |
//! | `releases/{n}/grids/{grid}/{field}.f32` | A grid field's samples, little-endian `f32` |
//!
//! The release directory also holds `d1/*.sql`, the release's D1 rows, which go to D1 rather
//! than R2. Publishing release n uploads only below `releases/{n}/`, so the current release's
//! keys are never written; pruning it deletes that prefix.

/// The prefix of every key of release `n`, with its closing slash.
#[must_use]
pub fn prefix(n: u32) -> String {
    format!("releases/{n}/")
}

/// The key of the file at `path` within release `n`'s directory.
#[must_use]
pub fn key(n: u32, path: &str) -> String {
    format!("{}{path}", prefix(n))
}

/// The release manifest's path within the release directory.
pub const MANIFEST: &str = "manifest.yaml";
/// The directory, within the release directory, of the D1 files.
pub const D1_DIR: &str = "d1";

/// The path of the rules file `name`, such as `matching`.
#[must_use]
pub fn rules(name: &str) -> String {
    format!("rules/{name}.yaml")
}

/// The path of a scale domain's tile manifest.
#[must_use]
pub fn tile_manifest(domain: &str) -> String {
    format!("tiles/{domain}/manifest.json")
}

/// The path of the tile of the node at `address`, such as `r07`.
#[must_use]
pub fn tile(domain: &str, address: &str) -> String {
    format!("tiles/{domain}/{address}.bin")
}

/// The path of a volume grid's header.
#[must_use]
pub fn grid_header(grid: &str) -> String {
    format!("grids/{grid}/header.json")
}

/// The path of a volume grid's samples file `name`.
#[must_use]
pub fn grid_samples(grid: &str, name: &str) -> String {
    format!("grids/{grid}/{name}.f32")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_lies_under_its_release() {
        assert_eq!(key(7, MANIFEST), "releases/7/manifest.yaml");
        assert_eq!(
            key(7, &tile("nearby_galaxies", "r07")),
            "releases/7/tiles/nearby_galaxies/r07.bin"
        );
        assert_eq!(
            key(12, &tile_manifest("stars")),
            "releases/12/tiles/stars/manifest.json"
        );
        assert_eq!(
            key(1, &grid_samples("cf4pp-density", "field-0")),
            "releases/1/grids/cf4pp-density/field-0.f32"
        );
        assert_eq!(key(1, &rules("ranking")), "releases/1/rules/ranking.yaml");
        assert!(!key(1, MANIFEST).starts_with(&prefix(10)));
    }
}
