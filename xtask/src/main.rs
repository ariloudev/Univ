//! Project tasks for univ-server.
//!
//! - `cargo xtask check` runs every gate in order and stops at the first that fails.
//! - `cargo xtask lines` reports each `.rs` file over the 500-line limit.

mod lines;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The most lines a `.rs` file may hold.
const LINE_LIMIT: usize = 500;

/// One gate of `cargo xtask check`.
enum Gate {
    /// A command run from the workspace root: its name, program and arguments.
    Command(&'static str, &'static str, &'static [&'static str]),
    /// The 500-line rule.
    Lines,
    /// `loft verify`, which only holds on the publish branch.
    Verify,
}

/// The gates, in the order they run.
const GATES: &[Gate] = &[
    Gate::Command("fmt", "cargo", &["fmt", "--all", "--check"]),
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
    Gate::Lines,
    Gate::Command("test", "cargo", &["test", "--workspace", "--quiet"]),
    Gate::Command("deny", "cargo", &["deny", "--log-level", "error", "check"]),
    Gate::Command("machete", "cargo-machete", &[]),
    Gate::Command(
        "wasm32",
        "cargo",
        &[
            "build",
            "--package",
            "univ-model",
            "--target",
            "wasm32-unknown-unknown",
            "--quiet",
        ],
    ),
    Gate::Command("loft fmt", "loft", &["fmt", "--check"]),
    Gate::Command("loft check", "loft", &["check", "--fail-on", "warning"]),
    Gate::Verify,
];

impl Gate {
    fn name(&self) -> &'static str {
        match self {
            Self::Command(name, _, _) => name,
            Self::Lines => "lines",
            Self::Verify => "loft verify",
        }
    }

    fn passes(&self, root: &Path) -> bool {
        match self {
            Self::Command(_, program, args) => run(root, program, args),
            Self::Lines => lines_pass(root),
            Self::Verify => verify(root),
        }
    }
}

fn main() -> ExitCode {
    let root = workspace_root();
    let passed = match std::env::args().nth(1).as_deref() {
        Some("check") => check(&root),
        Some("lines") => lines_pass(&root),
        _ => {
            eprintln!("usage: cargo xtask <check|lines>");
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
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

fn check(root: &Path) -> bool {
    for gate in GATES {
        eprintln!("==> {}", gate.name());
        if !gate.passes(root) {
            eprintln!("check failed at: {}", gate.name());
            return false;
        }
    }
    eprintln!("check passed: {} gates", GATES.len());
    true
}

fn run(root: &Path, program: &str, args: &[&str]) -> bool {
    match Command::new(program).args(args).current_dir(root).status() {
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
/// publish makes normative, so the tree is not expected to match the lock.
fn verify(root: &Path) -> bool {
    let publish = publish_branch(root);
    let current = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    if current == publish {
        run(root, "loft", &["verify"])
    } else {
        eprintln!("skipped: loft verify runs on the publish branch `{publish}`, not `{current}`");
        true
    }
}

/// The branch `spec/loftspec.yaml` names as `publishBranch`, or `main` when it names none.
fn publish_branch(root: &Path) -> String {
    fs::read_to_string(root.join("spec/loftspec.yaml"))
        .ok()
        .and_then(|manifest| {
            manifest.lines().find_map(|line| {
                line.strip_prefix("publishBranch:")
                    .map(|value| value.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "main".to_owned())
}
