//! The [`Store`] of local D1 and R2 through Wrangler's `--local` commands, against a
//! `--persist-to` state directory: what `wrangler dev` reads when given the same directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

use super::{QueryRow, Store};

/// The D1 database `wrangler.toml` names.
pub const DATABASE: &str = "univ";
/// The R2 bucket `wrangler.toml` names.
pub const BUCKET: &str = "univ-releases";

/// Wrangler, run from the repository's `node_modules`.
#[derive(Clone, Debug)]
pub struct Wrangler {
    program: PathBuf,
    config: PathBuf,
    persist_to: PathBuf,
    scratch: PathBuf,
}

impl Wrangler {
    /// Wrangler as `npm install` put it beside `config`, the repository's `wrangler.toml`,
    /// writing the local state in `persist_to`.
    ///
    /// # Errors
    ///
    /// Fails, saying how to install it, when the configuration or Wrangler is missing.
    pub fn new(config: &Path, persist_to: &Path) -> Result<Self, String> {
        if !config.is_file() {
            return Err(format!("{} does not exist", config.display()));
        }
        let root = config.parent().unwrap_or(Path::new("."));
        let program = root.join("node_modules").join(".bin").join("wrangler");
        if !program.is_file() {
            return Err(format!(
                "{} does not exist: run `npm install` in {}",
                program.display(),
                root.display()
            ));
        }
        let scratch = persist_to.join("univ-ingest");
        std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
        Ok(Self {
            program,
            config: config.to_path_buf(),
            persist_to: persist_to.to_path_buf(),
            scratch,
        })
    }

    /// Runs Wrangler with `args` and the local options.
    fn run(&self, args: &[&str]) -> Result<Output, String> {
        Command::new(&self.program)
            .args(args)
            .arg("--local")
            .arg("--persist-to")
            .arg(&self.persist_to)
            .arg("--config")
            .arg(&self.config)
            .env("WRANGLER_SEND_METRICS", "false")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|e| format!("{}: {e}", self.program.display()))
    }

    fn object(key: &str) -> String {
        format!("{BUCKET}/{key}")
    }
}

/// What a failed run said: its JSON error, or the last line of its error output.
fn failure(args: &[&str], output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json = serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .and_then(|v| v["error"]["text"].as_str().map(str::to_owned));
    let said = json.unwrap_or_else(|| {
        stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .find(|l| l.contains("ERROR") || l.starts_with('✘'))
            .unwrap_or("no message")
            .to_owned()
    });
    format!("wrangler {} failed: {said}", args.join(" "))
}

/// A MIME type for an R2 object, by its extension.
fn content_type(key: &str) -> &'static str {
    match key.rsplit_once('.').map(|(_, ext)| ext) {
        Some("json") => "application/json",
        Some("yaml") => "application/yaml",
        _ => "application/octet-stream",
    }
}

impl Store for Wrangler {
    fn execute_file(&mut self, file: &Path) -> Result<(), String> {
        let file = file.to_string_lossy();
        let args = ["d1", "execute", DATABASE, "--file", &file];
        let output = self.run(&args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure(&args, &output))
        }
    }

    fn query(&mut self, sql: &str) -> Result<Vec<QueryRow>, String> {
        let args = ["d1", "execute", DATABASE, "--command", sql, "--json"];
        let output = self.run(&args)?;
        if !output.status.success() {
            return Err(failure(&args, &output));
        }
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("wrangler d1 execute gave no JSON: {e}"))?;
        let results = value
            .as_array()
            .and_then(|a| a.last())
            .and_then(|r| r["results"].as_array())
            .ok_or("wrangler d1 execute gave no results")?;
        Ok(results
            .iter()
            .filter_map(|r| r.as_object().cloned())
            .collect())
    }

    fn put(&mut self, key: &str, file: &Path) -> Result<(), String> {
        let (object, file) = (Self::object(key), file.to_string_lossy());
        let args = [
            "r2",
            "object",
            "put",
            &object,
            "--file",
            &file,
            "--content-type",
            content_type(key),
        ];
        let output = self.run(&args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure(&args, &output))
        }
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let back = self.scratch.join("read-back");
        let _ = std::fs::remove_file(&back);
        let (object, file) = (Self::object(key), back.to_string_lossy().into_owned());
        let args = ["r2", "object", "get", &object, "--file", &file];
        let output = self.run(&args)?;
        if output.status.success() {
            let bytes = std::fs::read(&back).map_err(|e| format!("{}: {e}", back.display()))?;
            let _ = std::fs::remove_file(&back);
            return Ok(Some(bytes));
        }
        let _ = std::fs::remove_file(&back);
        let said = String::from_utf8_lossy(&output.stderr);
        if said.contains("The specified key does not exist") {
            Ok(None)
        } else {
            Err(failure(&args, &output))
        }
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        let object = Self::object(key);
        let args = ["r2", "object", "delete", &object];
        let output = self.run(&args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure(&args, &output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_those_of_the_repositorys_wrangler_toml() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let toml = std::fs::read_to_string(root.join("wrangler.toml")).unwrap();
        assert!(toml.contains(&format!("database_name = \"{DATABASE}\"")));
        assert!(toml.contains(&format!("bucket_name = \"{BUCKET}\"")));
        assert!(!toml.contains("main ="), "no Worker entry");
        assert_eq!(
            content_type("releases/1/tiles/stars/manifest.json"),
            "application/json"
        );
        assert_eq!(
            content_type("releases/1/tiles/stars/r0.bin"),
            "application/octet-stream"
        );
    }
}
