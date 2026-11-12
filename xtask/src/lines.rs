//! The 500-line rule: no `.rs` file in the repository may run longer than the limit. A file over
//! it is split by responsibility, never exempted.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every file git sees under `root` that matches one of `pathspecs`, tracked or new but not
/// ignored, as paths joined to `root`.
///
/// # Errors
///
/// Fails when git cannot list the files.
pub fn listed(root: &Path, pathspecs: &[&str]) -> io::Result<Vec<PathBuf>> {
    let listing = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .args(pathspecs)
        .current_dir(root)
        .output()?;
    if !listing.status.success() {
        let reason = String::from_utf8_lossy(&listing.stderr);
        return Err(io::Error::other(format!(
            "git ls-files failed: {}",
            reason.trim()
        )));
    }
    Ok(listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| root.join(String::from_utf8_lossy(name).as_ref()))
        .collect())
}

/// Every `.rs` file git sees under `root`, tracked or new but not ignored, with more than `limit`
/// lines, with its line count, in path order.
///
/// # Errors
///
/// Fails when git cannot list the files or a listed file cannot be read; the error names the file.
pub fn over_limit(root: &Path, limit: usize) -> io::Result<Vec<(PathBuf, usize)>> {
    let mut found = Vec::new();
    for path in listed(root, &["*.rs"])? {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            // A tracked file deleted from the working tree is still listed.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("{}: {error}", path.display()),
                ));
            }
        };
        let lines = line_count(&bytes);
        if lines > limit {
            found.push((path, lines));
        }
    }
    found.sort();
    Ok(found)
}

/// The number of lines in `bytes`, counting a last line without a newline, whatever the encoding.
fn line_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    bytes.split(|byte| *byte == b'\n').count() - usize::from(bytes.ends_with(b"\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// No file over the limit.
    const NONE: Vec<(PathBuf, usize)> = Vec::new();

    /// A new git repository in a temporary directory.
    fn repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status();
        assert!(init.unwrap().success());
        dir
    }

    fn write(repo: &TempDir, relative: &str, contents: &[u8]) -> PathBuf {
        let path = repo.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn write_lines(repo: &TempDir, relative: &str, lines: usize) -> PathBuf {
        write(repo, relative, "x\n".repeat(lines).as_bytes())
    }

    #[test]
    fn a_file_at_the_limit_passes() {
        let repo = repo();
        write_lines(&repo, "src/ok.rs", 500);
        assert_eq!(over_limit(repo.path(), 500).unwrap(), NONE);
    }

    #[test]
    fn a_new_file_over_the_limit_is_reported_with_its_count() {
        let repo = repo();
        let long = write_lines(&repo, "crates/a/src/long.rs", 501);
        write_lines(&repo, "crates/a/src/short.rs", 10);
        assert_eq!(over_limit(repo.path(), 500).unwrap(), vec![(long, 501)]);
    }

    #[test]
    fn files_git_ignores_are_skipped_but_a_module_named_target_is_not() {
        let repo = repo();
        write(&repo, ".gitignore", b"/target\n");
        write_lines(&repo, "target/debug/build/generated.rs", 900);
        let module = write_lines(&repo, "src/target/mod.rs", 900);
        assert_eq!(over_limit(repo.path(), 500).unwrap(), vec![(module, 900)]);
    }

    #[test]
    fn only_rust_files_are_counted() {
        let repo = repo();
        write_lines(&repo, "docs/notes.md", 900);
        write_lines(&repo, "sources/sdss.yaml", 900);
        assert_eq!(over_limit(repo.path(), 500).unwrap(), NONE);
    }

    #[test]
    fn a_file_that_is_not_utf8_is_still_counted() {
        let repo = repo();
        let mut latin1 = b"// caf\xe9\n".repeat(501);
        latin1.pop();
        let path = write(&repo, "src/latin1.rs", &latin1);
        assert_eq!(over_limit(repo.path(), 500).unwrap(), vec![(path, 501)]);
    }
}
