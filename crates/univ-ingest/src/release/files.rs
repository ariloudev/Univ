//! A release's directory: its files written as they come, its manifest, and the files read
//! back and checked against it.

use std::path::{Path, PathBuf};

use univ_model::release::keys;
use univ_model::release::manifest::{ManifestFile, ReleaseManifest, Target};

use super::check::{Failed, MANIFEST};
use super::d1::PartSink;
use crate::tiles::sha256;
use crate::watch::Watch;
use crate::yaml::{FileError, parse_checked};

/// The directory, within the build output, of the releases.
pub const RELEASES: &str = "releases";

/// The directory of release `number` under the build output `out`.
#[must_use]
pub fn release_dir(out: &Path, number: u32) -> PathBuf {
    out.join(RELEASES).join(number.to_string())
}

/// A release directory written as its files come, each listed with its size and SHA-256 as the
/// manifest lists it: a D1 file by its path under `d1/`, any other by its R2 key too. Each file
/// is a safe point of the build's `watch`: once the owner asks the build to stop, the next file
/// is refused.
pub struct DirSink<'w> {
    dir: PathBuf,
    number: u32,
    pub(super) watch: Watch<'w>,
    pub files: Vec<ManifestFile>,
}

impl<'w> DirSink<'w> {
    /// A new, empty directory `dir` for release `number`, replacing what a stopped build left.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be made.
    pub fn new(dir: &Path, number: u32, watch: Watch<'w>) -> Result<Self, String> {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Self {
            dir: dir.to_owned(),
            number,
            watch,
            files: Vec::new(),
        })
    }
}

impl PartSink for DirSink<'_> {
    fn put(&mut self, path: String, bytes: &[u8]) -> Result<(), String> {
        self.watch.check()?;
        let full = self.dir.join(&path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| format!("{} cannot be written: {e}", full.display()))?;
        let target = if path.starts_with(&format!("{}/", keys::D1_DIR)) {
            Target::D1
        } else {
            Target::R2
        };
        self.files.push(ManifestFile {
            key: (target == Target::R2).then(|| keys::key(self.number, &path)),
            path,
            target,
            bytes: bytes.len() as u64,
            sha256: sha256(bytes),
        });
        Ok(())
    }
}

/// Writes `manifest` into the release directory `dir`, beside the files it lists.
///
/// # Errors
///
/// Fails, as the manifest check, when it cannot be written.
pub fn write_manifest(dir: &Path, manifest: &ReleaseManifest) -> Result<(), Failed> {
    let path = dir.join(keys::MANIFEST);
    crate::yaml::write(&path, manifest).map_err(|e| Failed {
        check: MANIFEST,
        cause: format!("{} cannot be written: {e}", path.display()),
    })
}

/// The manifest of release `number` as `text`, read from `origin`, checked to be of that
/// release.
///
/// # Errors
///
/// Fails, naming where it came from and the field, when it does not parse as a manifest or is
/// another release's.
pub fn parse_manifest(
    origin: &Path,
    text: &str,
    number: u32,
) -> Result<ReleaseManifest, FileError> {
    parse_checked(origin, text, |m: &ReleaseManifest| {
        if m.release == number {
            Ok(())
        } else {
            Err((
                "release".to_owned(),
                format!("is {}, not {number}", m.release),
            ))
        }
    })
}

/// The manifest of release `number` in its release directory `dir`, with its bytes as read.
///
/// # Errors
///
/// As [`parse_manifest`], and when the file cannot be read.
pub fn read_manifest(dir: &Path, number: u32) -> Result<(ReleaseManifest, Vec<u8>), FileError> {
    let path = dir.join(keys::MANIFEST);
    let bytes = std::fs::read(&path).map_err(|e| FileError::new(&path, "(file)", e.to_string()))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|e| FileError::new(&path, "(file)", e.to_string()))?;
    let manifest = parse_manifest(&path, text, number)?;
    Ok((manifest, bytes))
}

/// Reads every file the manifest lists back from `dir` and checks its size and SHA-256, and
/// that the directory holds no other file.
///
/// # Errors
///
/// Fails, as the manifest check, naming the first file that differs.
pub fn verify(dir: &Path, manifest: &ReleaseManifest) -> Result<(), Failed> {
    let fail = |cause: String| Failed {
        check: MANIFEST,
        cause,
    };
    for f in &manifest.files {
        let bytes =
            std::fs::read(dir.join(&f.path)).map_err(|e| fail(format!("{}: {e}", f.path)))?;
        if bytes.len() as u64 != f.bytes || sha256(&bytes) != f.sha256 {
            return Err(fail(format!("{} does not match its SHA-256", f.path)));
        }
    }
    let on_disk = list(dir, dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
    let listed = manifest.files.len() + 1;
    if on_disk.len() != listed {
        return Err(fail(format!(
            "the directory holds {} files, the manifest lists {listed} with itself",
            on_disk.len()
        )));
    }
    Ok(())
}

/// Every file under `dir`, by its path relative to `root`, sorted.
///
/// # Errors
///
/// Fails when a directory cannot be read.
pub fn list(root: &Path, dir: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(list(root, &path)?);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    Ok(out)
}
