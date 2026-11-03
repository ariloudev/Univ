//! Built tiles written to an output directory: `<out>/<domain>/<address>.bin` for each node and
//! `<out>/<domain>/manifest.json`. Each file is written whole through a `.part` file and a
//! rename. The manifest an earlier build left in a domain's directory is removed first, then
//! its tiles and any partial file, and the manifest is written last: a domain whose manifest
//! is there has every tile it names. Nothing else in the directory is touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Built, name_of};
use crate::fetch::write_via_part;

/// The manifest's file name in a domain's directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// The file of the tile at `address` in a domain's directory.
#[must_use]
pub fn tile_file(address: &str) -> String {
    format!("{address}.bin")
}

/// Whether `name` is a file a build writes: the manifest or a tile, `r` then octant digits, or
/// either's partial file.
fn written_by_a_build(name: &str) -> bool {
    let name = name.strip_suffix(".part").unwrap_or(name);
    name == MANIFEST_FILE
        || name.strip_suffix(".bin").is_some_and(|a| {
            a.strip_prefix('r')
                .is_some_and(|digits| digits.bytes().all(|b| (b'0'..=b'7').contains(&b)))
        })
}

/// Writes every domain of `built` under `out`, giving each domain's directory.
///
/// # Errors
///
/// Fails when a directory or file cannot be removed, made or written, naming it.
pub fn write_all(out: &Path, built: &Built) -> Result<Vec<PathBuf>, String> {
    let named = |path: &Path, e: io::Error| format!("{}: {e}", path.display());
    let mut dirs = Vec::new();
    for domain in &built.domains {
        let dir = out.join(name_of(domain.domain));
        fs::create_dir_all(&dir).map_err(|e| named(&dir, e))?;
        let manifest = dir.join(MANIFEST_FILE);
        if manifest.is_file() {
            fs::remove_file(&manifest).map_err(|e| named(&manifest, e))?;
        }
        for entry in fs::read_dir(&dir).map_err(|e| named(&dir, e))? {
            let path = entry.map_err(|e| named(&dir, e))?.path();
            let stale = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(written_by_a_build);
            if stale && path.is_file() {
                fs::remove_file(&path).map_err(|e| named(&path, e))?;
            }
        }
        for node in &domain.nodes {
            let path = dir.join(tile_file(&node.address));
            write_via_part(&path, &node.tile, || Ok(())).map_err(|e| named(&path, e))?;
        }
        write_via_part(&manifest, &domain.json, || Ok(())).map_err(|e| named(&manifest, e))?;
        dirs.push(dir);
    }
    Ok(dirs)
}
