//! The repository as the CLI tests run Univ Ingest on it: every entry of the workspace linked,
//! but a Galaxy model of the fixture's size, since the production definition's 10 million points
//! take minutes to sample in a debug build.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use univ_ingest::tiles::sha256;

use super::workspace;

/// The points of the CLI tests' Galaxy model.
pub const MODEL_POINTS: u64 = 20_000;

/// The production definition's line giving its points.
const POINTS_LINE: &str = "points: 10000000\n";

/// The mirror, made once per test binary.
static MIRROR: LazyLock<PathBuf> = LazyLock::new(|| mirror().expect("the mirror is made"));

/// The repository of the CLI tests.
pub fn test_repository() -> &'static Path {
    &MIRROR
}

/// Links each entry of `from` into `to`, but `except`.
fn link_all(from: &Path, to: &Path, except: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let name = entry?.file_name();
        if name != except {
            std::os::unix::fs::symlink(from.join(&name), to.join(&name))?;
        }
    }
    Ok(())
}

/// Makes the mirror under the tests' scratch directory, named by what it holds, so that test
/// binaries running at once share it and a changed definition or rules folder makes another.
/// It is made under a name of its own and renamed into place: a binary that loses the race
/// takes the one that won.
fn mirror() -> std::io::Result<PathBuf> {
    let root = workspace();
    let rules = root.join("rules");
    let model = std::fs::read_to_string(rules.join("galaxy-model.yaml"))?;
    assert!(model.contains(POINTS_LINE), "the definition asks for 10M");
    let small = model.replacen(POINTS_LINE, &format!("points: {MODEL_POINTS}\n"), 1);
    let mut names: Vec<_> = std::fs::read_dir(&rules)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    names.sort();
    let id = sha256(format!("{small}{}", names.join("\n")).as_bytes());
    let id = &id[..16];
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let target = scratch.join(format!("repository-{id}"));
    if target.is_dir() {
        return Ok(target);
    }
    let making = tempfile::Builder::new()
        .prefix("repository-making-")
        .tempdir_in(scratch)?;
    link_all(&root, making.path(), "rules")?;
    link_all(&rules, &making.path().join("rules"), "galaxy-model.yaml")?;
    std::fs::write(making.path().join("rules/galaxy-model.yaml"), small)?;
    match std::fs::rename(making.path(), &target) {
        Ok(()) => {
            // Renamed away: nothing is left for the temporary directory to remove.
            let _ = making.keep();
            Ok(target)
        }
        Err(_) if target.is_dir() => Ok(target),
        Err(e) => Err(e),
    }
}

/// The repository's Galaxy model, at the CLI tests' size.
pub fn repository_model() -> univ_model::galaxy_model::Model {
    let file = workspace().join("rules/galaxy-model.yaml");
    let mut model = univ_ingest::galaxy_model::ModelFile::read(&file)
        .expect("the definition reads")
        .model()
        .expect("the definition gives a model");
    model.points = MODEL_POINTS;
    model
}
