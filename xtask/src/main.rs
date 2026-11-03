//! Project tasks for univ-server.
//!
//! - `cargo xtask check` runs every gate in order and stops at the first that fails.
//! - `cargo xtask lines` reports each `.rs` file over the 500-line limit.
//! - `cargo xtask science <release-dir>` runs the science checks of a built release.
//! - `cargo xtask integration` runs Univ Server's end-to-end tests inside `wrangler dev`.
//! - `cargo xtask serve` runs Univ Server on the real local state.
//! - `cargo xtask bench lookup` times object lookups through it (QA-2).

mod bench;
mod integration;
mod lines;
mod node;
mod science;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The most lines a `.rs` file may hold.
const LINE_LIMIT: usize = 500;

/// Variables `cargo run` sets for xtask itself. A child that inherits them can take itself for
/// xtask: cargo-machete stopped recognising it ran as `cargo machete` when `CARGO_PKG_NAME` was set.
const CARGO_RUN_VARIABLES: &[&str] = &[
    "CARGO_PKG_",
    "CARGO_MANIFEST_",
    "CARGO_BIN_",
    "CARGO_CRATE_NAME",
    "CARGO_PRIMARY_PACKAGE",
];

/// One gate of `cargo xtask check`.
enum Gate {
    /// A command run from the workspace root: its name, program and arguments.
    Command(&'static str, &'static str, &'static [&'static str]),
    /// The 500-line rule.
    Lines,
    /// `loft verify`, which only holds on the publish branch.
    Verify,
    /// The reference TypeScript tile decoder's tests, when `node` is on the path.
    TileDecoder,
}

/// What a gate came to.
#[derive(PartialEq, Eq)]
pub(crate) enum Outcome {
    Passed,
    Skipped,
    Failed,
}

/// The gates, in the order they run: the quick ones before the ones that compile the workspace.
const GATES: &[Gate] = &[
    Gate::Command("fmt", "cargo", &["fmt", "--all", "--check"]),
    Gate::Lines,
    Gate::Command("machete", "cargo", &["machete"]),
    Gate::Command("loft fmt", "loft", &["fmt", "--check"]),
    Gate::Command("loft check", "loft", &["check", "--fail-on", "warning"]),
    Gate::Command(
        "loft reconcile",
        "loft",
        &["reconcile", "--fail-on", "warning"],
    ),
    Gate::Command(
        "clippy",
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--quiet",
            "--",
            "-D",
            "warnings",
        ],
    ),
    Gate::Command("test", "cargo", &["test", "--workspace", "--quiet"]),
    Gate::TileDecoder,
    Gate::Command(
        "wasm32",
        "cargo",
        &[
            "check",
            "--package",
            "univ-model",
            "--package",
            "univ-api",
            "--target",
            "wasm32-unknown-unknown",
            "--quiet",
        ],
    ),
    // The Worker's own target: what native clippy cannot see, such as code behind
    // `cfg(target_arch = "wasm32")` in workers-rs's macros.
    Gate::Command(
        "clippy wasm32",
        "cargo",
        &[
            "clippy",
            "--package",
            "univ-server",
            "--target",
            "wasm32-unknown-unknown",
            "--quiet",
            "--",
            "-D",
            "warnings",
        ],
    ),
    Gate::Command("deny", "cargo", &["deny", "check"]),
    Gate::Verify,
];

impl Gate {
    fn name(&self) -> &'static str {
        match self {
            Self::Command(name, _, _) => name,
            Self::Lines => "lines",
            Self::Verify => "loft verify",
            Self::TileDecoder => "tile decoder",
        }
    }

    fn run(&self, root: &Path) -> Outcome {
        let passed = match self {
            Self::Command(_, program, args) => run(root, program, args),
            Self::Lines => lines_pass(root),
            Self::Verify => return verify(root),
            Self::TileDecoder => return node::tile_decoder(root),
        };
        if passed {
            Outcome::Passed
        } else {
            Outcome::Failed
        }
    }
}

fn main() -> ExitCode {
    let root = workspace_root();
    let passed = match std::env::args().nth(1).as_deref() {
        Some("check") => check(&root),
        Some("lines") => lines_pass(&root),
        Some("integration") => {
            integration::stop_on_interrupt();
            integration::integration(&root)
        }
        Some("serve") => integration::serve(&root),
        Some("bench") if std::env::args().nth(2).as_deref() == Some("lookup") => {
            integration::stop_on_interrupt();
            bench::lookup(&root)
        }
        Some("science") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            science::run(&root, &args)
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <check|lines|integration|serve|bench lookup|science <release-dir>>"
            );
            return ExitCode::from(2);
        }
    };
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The workspace root: the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).to_path_buf()
}

fn check(root: &Path) -> bool {
    let mut skipped = Vec::new();
    for gate in GATES {
        eprintln!("==> {}", gate.name());
        match gate.run(root) {
            Outcome::Passed => {}
            Outcome::Skipped => skipped.push(gate.name()),
            Outcome::Failed => {
                eprintln!("check failed at: {}", gate.name());
                return false;
            }
        }
    }
    let passed = GATES.len() - skipped.len();
    if skipped.is_empty() {
        eprintln!("check passed: {passed} gates");
    } else {
        eprintln!(
            "check passed: {passed} gates, skipped: {}",
            skipped.join(", ")
        );
    }
    true
}

/// A command for `program` run from the workspace root, without the variables `cargo run` set.
pub(crate) fn command(root: &Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command.current_dir(root);
    for (key, _) in std::env::vars_os() {
        let set_by_cargo_run = key.to_str().is_some_and(|key| {
            CARGO_RUN_VARIABLES
                .iter()
                .any(|prefix| key.starts_with(prefix))
        });
        if set_by_cargo_run {
            command.env_remove(&key);
        }
    }
    command
}

fn run(root: &Path, program: &str, args: &[&str]) -> bool {
    match command(root, program).args(args).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("could not run {program}: {error}");
            false
        }
    }
}

fn lines_pass(root: &Path) -> bool {
    match lines::over_limit(root, LINE_LIMIT) {
        Ok(found) if found.is_empty() => true,
        Ok(found) => {
            for (path, count) in found {
                let shown = path.strip_prefix(root).unwrap_or(&path).display();
                eprintln!("{shown}: {count} lines, over the {LINE_LIMIT}-line limit; split it");
            }
            false
        }
        Err(error) => {
            eprintln!("could not count lines: {error}");
            false
        }
    }
}

/// Runs `loft verify` on the publish branch only: elsewhere `spec/` may hold changes that the next
/// publish makes normative, so the tree is not expected to match the lock. A branch or publish
/// branch that cannot be read fails the gate rather than skipping it.
fn verify(root: &Path) -> Outcome {
    let publish = match publish_branch(root) {
        Ok(branch) => branch,
        Err(error) => {
            eprintln!("could not read the publish branch: {error}");
            return Outcome::Failed;
        }
    };
    let current = match current_branch(root) {
        Ok(branch) => branch,
        Err(error) => {
            eprintln!("could not tell the current branch: {error}");
            return Outcome::Failed;
        }
    };
    if current == publish {
        if run(root, "loft", &["verify"]) {
            Outcome::Passed
        } else {
            Outcome::Failed
        }
    } else {
        eprintln!("skipped: loft verify runs on the publish branch `{publish}`, not `{current}`");
        Outcome::Skipped
    }
}

/// The branch checked out, refusing a detached HEAD, where it cannot be told.
fn current_branch(root: &Path) -> Result<String, String> {
    let output = command(root, "git")
        .args(["branch", "--show-current"])
        .output()
        .map_err(|error| format!("git: {error}"))?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    } else if branch.is_empty() {
        Err("HEAD is detached; check out a branch".to_owned())
    } else {
        Ok(branch)
    }
}

/// The branch `spec/loftspec.yaml` names as `publishBranch`, or `main` when it names none.
fn publish_branch(root: &Path) -> Result<String, String> {
    let path = root.join("spec/loftspec.yaml");
    let manifest =
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(manifest
        .lines()
        .find_map(|line| line.strip_prefix("publishBranch:").map(scalar))
        .unwrap_or_else(|| "main".to_owned()))
}

/// A plain YAML scalar as written after its key: a trailing comment and quotes removed.
fn scalar(raw: &str) -> String {
    let value = raw.split(" #").next().unwrap_or(raw).trim();
    value
        .trim_matches(|quote| quote == '"' || quote == '\'')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::scalar;

    #[test]
    fn a_publish_branch_is_read_plain_quoted_or_commented() {
        assert_eq!(scalar(" develop"), "develop");
        assert_eq!(scalar(" \"develop\""), "develop");
        assert_eq!(scalar(" 'develop'"), "develop");
        assert_eq!(scalar(" develop  # normative"), "develop");
    }
}
