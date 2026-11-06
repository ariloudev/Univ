//! Where a release's files lie, on local disk and in R2 (schema 6). A local build writes each
//! file at its path below its release directory; R2 keeps each file once, under its SHA-256,
//! `files/{sha256}`, shared by every release that holds it, and each release maps its paths to
//! those hashes in D1 (`release_files`). Only the release manifest is kept under the release's
//! own prefix, `releases/{n}/manifest.yaml`.
//!
//! | Path in the release directory | What it holds |
//! |---|---|
//! | `manifest.yaml` | The release manifest: every other file with its SHA-256 |
//! | `rules/{name}.yaml` | The matching, ranking, tile and release rules, verbatim |
//! | `tiles/{domain}/manifest.json` | A scale domain's tile manifest |
//! | `tiles/{domain}/{address}.bin` | One octree node's tile |
//! | `grids/{grid}/header.json` | A volume grid's header |
//! | `grids/{grid}/{field}.f32` | A grid field's samples, little-endian `f32` |
//!
//! The release directory also holds `d1/*.sql`, the release's D1 rows, which go to D1 rather
//! than R2. A file once in R2 never changes, since its key is what it holds.

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

/// The R2 key of the file whose SHA-256 is `sha256`, in lower-case hex.
#[must_use]
pub fn file(sha256: &str) -> String {
    format!("files/{sha256}")
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
    fn a_file_lies_under_its_hash_and_a_manifest_under_its_release() {
        assert_eq!(file("ab01"), "files/ab01");
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
