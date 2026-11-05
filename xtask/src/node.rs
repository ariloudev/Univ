//! The gate of the reference TypeScript tile decoder, `clients/tile-decoder`: its tests under
//! `node --test`, which Node runs from the `.ts` files directly by stripping their types. It is
//! skipped when `node` is not on the path, and fails when Node is too old to strip types.

use std::io::ErrorKind;
use std::path::Path;

use crate::{Outcome, command};

/// The decoder's directory, relative to the workspace.
const DIR: &str = "clients/tile-decoder";

/// The first Node that strips TypeScript types, behind `--experimental-strip-types`.
const MINIMUM: (u32, u32) = (22, 6);

/// The first Node that strips them without the flag.
const UNFLAGGED: (u32, u32) = (23, 6);

/// The major and minor version of `node --version`'s answer, such as `v24.12.0`.
fn parse(answer: &str) -> Option<(u32, u32)> {
    let mut parts = answer.trim().strip_prefix('v')?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Runs the tile decoder's tests.
pub fn tile_decoder(root: &Path) -> Outcome {
    let answer = match command(root, "node").arg("--version").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        Ok(output) => {
            eprintln!(
                "node --version failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Outcome::Failed;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!("skipped: node is not on the path, so {DIR}'s tests did not run");
            return Outcome::Skipped;
        }
        Err(error) => {
            eprintln!("could not run node: {error}");
            return Outcome::Failed;
        }
    };
    let Some(version) = parse(&answer) else {
        eprintln!(
            "node --version answered {:?}, not vMAJOR.MINOR.PATCH",
            answer.trim()
        );
        return Outcome::Failed;
    };
    if version < MINIMUM {
        eprintln!(
            "node {} is older than {}.{}, the first that runs TypeScript by stripping its \
             types; {DIR}'s tests need Node {}.{} or later (23.6 or later without a flag)",
            answer.trim(),
            MINIMUM.0,
            MINIMUM.1,
            MINIMUM.0,
            MINIMUM.1,
        );
        return Outcome::Failed;
    }
    let mut node = command(&root.join(DIR), "node");
    if version < UNFLAGGED {
        node.arg("--experimental-strip-types");
    }
    // Node expands the pattern itself.
    match node.args(["--test", "*.test.ts"]).status() {
        Ok(status) if status.success() => Outcome::Passed,
        Ok(_) => Outcome::Failed,
        Err(error) => {
            eprintln!("could not run node: {error}");
            Outcome::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM, parse};

    #[test]
    fn a_node_version_is_read_and_compared_by_major_then_minor() {
        assert_eq!(parse("v24.12.0\n"), Some((24, 12)));
        assert_eq!(parse("v22.6.0"), Some((22, 6)));
        assert_eq!(parse("24.12.0"), None);
        assert_eq!(parse("vtwenty"), None);
        assert!(parse("v22.5.1").unwrap() < MINIMUM);
        assert!(parse("v22.10.0").unwrap() >= MINIMUM);
        assert!(parse("v23.0.0").unwrap() >= MINIMUM);
    }
}
