//! The 500-line rule: no `.rs` file in the repository may run longer than the limit. A file over
//! it is split by responsibility, never exempted.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Directories never searched, besides hidden ones: build output and installed packages.
const SKIPPED: &[&str] = &["target", "node_modules"];

/// Every `.rs` file under `root` with more than `limit` lines, with its line count, in path order.
///
/// # Errors
///
/// Fails when a directory cannot be listed or a file cannot be read.
pub fn over_limit(root: &Path, limit: usize) -> io::Result<Vec<(PathBuf, usize)>> {
    let mut found = Vec::new();
    visit(root, limit, &mut found)?;
    found.sort();
    Ok(found)
}

fn visit(dir: &Path, limit: usize, found: &mut Vec<(PathBuf, usize)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_ref()) {
                visit(&path, limit, found)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let lines = fs::read_to_string(&path)?.lines().count();
            if lines > limit {
                found.push((path, lines));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No file over the limit.
    const NONE: Vec<(PathBuf, usize)> = Vec::new();

    /// A directory under the system's temporary directory, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("xtask-lines-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative: &str, lines: usize) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "x\n".repeat(lines)).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_file_at_the_limit_passes() {
        let dir = TempDir::new("at-limit");
        dir.write("src/ok.rs", 500);
        assert_eq!(over_limit(&dir.0, 500).unwrap(), NONE);
    }

    #[test]
    fn a_file_over_the_limit_is_reported_with_its_count() {
        let dir = TempDir::new("over-limit");
        let long = dir.write("crates/a/src/long.rs", 501);
        dir.write("crates/a/src/short.rs", 10);
        assert_eq!(over_limit(&dir.0, 500).unwrap(), vec![(long, 501)]);
    }

    #[test]
    fn build_output_and_hidden_directories_are_skipped() {
        let dir = TempDir::new("skipped");
        dir.write("target/debug/build/generated.rs", 900);
        dir.write(".git/hooks/odd.rs", 900);
        dir.write("node_modules/pkg/odd.rs", 900);
        assert_eq!(over_limit(&dir.0, 500).unwrap(), NONE);
    }

    #[test]
    fn only_rust_files_are_counted() {
        let dir = TempDir::new("rust-only");
        dir.write("docs/notes.md", 900);
        dir.write("sources/sdss.yaml", 900);
        assert_eq!(over_limit(&dir.0, 500).unwrap(), NONE);
    }
}
