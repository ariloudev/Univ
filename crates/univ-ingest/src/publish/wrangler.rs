//! The [`Store`] of local D1 and R2 against a `--persist-to` state directory: what
//! `wrangler dev` reads when given the same directory. D1 goes through Wrangler's `--local`
//! commands; every R2 operation of a run goes through one [`R2Batch`], started at the first.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

use univ_model::release::shards::Database;

use super::r2batch::R2Batch;
use super::{QueryRow, ReadBack, Store};

/// The catalogue database `wrangler.toml` names.
pub const DATABASE: &str = "univ";

/// The name `wrangler.toml` gives the D1 database `database`: the catalogue's [`DATABASE`], or
/// `univ-objects-{i}` for object shard `i`, bound as `OBJECTS_{i}`.
#[must_use]
pub fn database_name(database: Database) -> String {
    match database {
        Database::Catalogue => DATABASE.to_owned(),
        Database::Shard(i) => format!("univ-objects-{i}"),
    }
}

/// How many object shards the configuration `toml` binds: `OBJECTS_0` onwards, each named as
/// [`database_name`] names it, until the first missing.
#[must_use]
pub fn bound_shards(toml: &str) -> u32 {
    let mut bound = 0;
    loop {
        let shard = Database::Shard(bound);
        let binding = format!("binding = \"{}\"", shard.binding());
        let name = format!("database_name = \"{}\"", database_name(shard));
        let declared = toml.split("[[d1_databases]]").skip(1).any(|block| {
            let block = block.split("[[").next().unwrap_or(block);
            block.contains(&binding) && block.contains(&name)
        });
        if !declared {
            return bound;
        }
        bound += 1;
    }
}
/// The R2 bucket `wrangler.toml` names.
pub const BUCKET: &str = "univ-releases";
/// The most lines of Wrangler's standard error a failure gives: its last.
pub const STDERR_LINES: usize = 20;

/// The directory in Wrangler's local state directory `persist_to` that Univ Ingest keeps its
/// own files in: the publish journals and the scratch file an R2 object is read into.
#[must_use]
pub fn ingest_dir(persist_to: &Path) -> PathBuf {
    persist_to.join("univ-ingest")
}

/// Wrangler, run from the repository's `node_modules`, and the R2 batch script beside it.
#[derive(Debug)]
pub struct Wrangler {
    program: PathBuf,
    config: PathBuf,
    /// The configuration's directory, which Wrangler runs in and keeps its own cache under.
    root: PathBuf,
    persist_to: PathBuf,
    scratch: PathBuf,
    /// The R2 batch script, once an R2 operation started it.
    r2: Option<R2Batch>,
    /// How many times the R2 batch script was started.
    batches: usize,
}

/// `path` made absolute, since Wrangler runs in the configuration's directory.
fn absolute(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))
}

impl Wrangler {
    /// Wrangler as `npm install` put it beside `config`, the repository's `wrangler.toml`,
    /// writing the local state in `persist_to`. Wrangler runs in the configuration's directory,
    /// where it keeps its cache in `.wrangler/`.
    ///
    /// # Errors
    ///
    /// Fails, saying how to install it, when the configuration or Wrangler is missing.
    pub fn new(config: &Path, persist_to: &Path) -> Result<Self, String> {
        if !config.is_file() {
            return Err(format!("{} does not exist", config.display()));
        }
        let (config, persist_to) = (absolute(config)?, absolute(persist_to)?);
        let root = config.parent().unwrap_or(Path::new("/")).to_path_buf();
        let program = root.join("node_modules").join(".bin").join("wrangler");
        if !program.is_file() {
            return Err(format!(
                "{} does not exist: run `npm install` in {}",
                program.display(),
                root.display()
            ));
        }
        let scratch = ingest_dir(&persist_to);
        std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
        Ok(Self {
            program,
            config,
            root,
            persist_to,
            scratch,
            r2: None,
            batches: 0,
        })
    }

    /// How many times this store started the R2 batch script: once for a run that touched R2.
    #[must_use]
    pub fn r2_batches(&self) -> usize {
        self.batches
    }

    /// The R2 batch script, started at the first R2 operation.
    fn r2(&mut self) -> Result<&mut R2Batch, String> {
        if self.r2.is_none() {
            self.r2 = Some(R2Batch::start(&self.root, &self.persist_to)?);
            self.batches += 1;
        }
        self.r2
            .as_mut()
            .ok_or_else(|| "the R2 batch script did not start".to_owned())
    }

    /// Runs Wrangler with `args` and the local options.
    fn run(&self, args: &[&str]) -> Result<Output, String> {
        own_group(&mut Command::new(&self.program))
            .current_dir(&self.root)
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

    /// Runs Wrangler with `args` and gives its output when it succeeds, else its failure.
    fn run_ok(&self, args: &[&str]) -> Result<Output, String> {
        let output = self.run(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(failure(args, &output))
        }
    }
}

/// `command` in a process group of its own, so that the owner's Ctrl-C reaches only Univ Ingest,
/// which stops once the unit in hand is done rather than have Wrangler killed part way through
/// it.
pub(super) fn own_group(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    command
}

/// `text` without its ANSI escape sequences: control sequences (`ESC [` … a final byte),
/// operating system commands (`ESC ]` … BEL or `ESC \`) and two-byte escapes.
#[must_use]
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// What a failed run of `args` said, its escapes stripped: the JSON error it printed, if any,
/// and the last [`STDERR_LINES`] lines of its standard error.
fn failure(args: &[&str], output: &Output) -> String {
    said(
        args,
        &output.status.to_string(),
        &output.stdout,
        &output.stderr,
    )
}

fn said(args: &[&str], status: &str, stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = strip_ansi(&String::from_utf8_lossy(stdout));
    let stderr = strip_ansi(&String::from_utf8_lossy(stderr));
    let mut said = format!("wrangler {} failed ({status})", args.join(" "));
    let json = serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .and_then(|v| v["error"]["text"].as_str().map(str::to_owned));
    if let Some(json) = json {
        said.push_str(": ");
        said.push_str(&json);
    }
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(STDERR_LINES)..];
    if tail.is_empty() {
        said.push_str("; it wrote nothing to standard error");
    } else {
        let last = if tail.len() < lines.len() {
            format!(", its last {STDERR_LINES} lines")
        } else {
            String::new()
        };
        said.push_str("; its standard error");
        said.push_str(&last);
        said.push_str(":\n");
        said.push_str(&tail.join("\n"));
    }
    said
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
    fn execute_file(&mut self, database: Database, file: &Path) -> Result<(), String> {
        let file = absolute(file)?;
        let file = file.to_string_lossy();
        let name = database_name(database);
        self.run_ok(&["d1", "execute", &name, "--file", &file])
            .map(drop)
    }

    fn query(&mut self, database: Database, sql: &str) -> Result<Vec<QueryRow>, String> {
        let name = database_name(database);
        let output = self.run_ok(&["d1", "execute", &name, "--command", sql, "--json"])?;
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

    fn bound_shards(&mut self) -> Result<u32, String> {
        let toml = std::fs::read_to_string(&self.config)
            .map_err(|e| format!("{}: {e}", self.config.display()))?;
        Ok(bound_shards(&toml))
    }

    fn put(&mut self, key: &str, file: &Path) -> Result<(), String> {
        self.put_read_back(key, file).map(drop)
    }

    fn put_read_back(&mut self, key: &str, file: &Path) -> Result<Option<ReadBack>, String> {
        let file = absolute(file)?;
        self.r2()?.put(key, &file, content_type(key))
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let back = self.scratch.join("read-back");
        let _ = std::fs::remove_file(&back);
        let found = self.r2()?.get(key, &back)?;
        let bytes = if found {
            Some(std::fs::read(&back).map_err(|e| format!("{}: {e}", back.display()))?)
        } else {
            None
        };
        let _ = std::fs::remove_file(&back);
        Ok(bytes)
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        self.r2()?.delete(&[key.to_owned()])
    }

    fn delete_keys(&mut self, keys: &[String]) -> Result<(), String> {
        self.r2()?.delete(keys)
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
        assert_eq!(bound_shards(&toml), 8);
        // Univ Server binds the same databases, by the same ids.
        let server =
            std::fs::read_to_string(root.join("crates/univ-server/wrangler.toml")).unwrap();
        assert_eq!(bound_shards(&server), 8);
        // Each database's name and id lines, the Worker's own univ-layers left out.
        let ids = |toml: &str| -> Vec<String> {
            toml.split("[[")
                .filter(|b| b.starts_with("d1_databases]]") && !b.contains("univ-layers"))
                .flat_map(|b| {
                    b.lines()
                        .filter(|l| l.starts_with("database_"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert_eq!(ids(&toml), ids(&server));
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

    #[test]
    fn a_failure_gives_wranglers_standard_error_without_its_escapes() {
        // Recorded: `wrangler d1 execute --command "SELEC 1"`, colours forced.
        let recorded =
            include_bytes!("../../tests/fixtures/wrangler/d1-execute-syntax-error.stderr");
        assert!(recorded.contains(&0x1b), "the recording holds escapes");
        let args = ["d1", "execute", DATABASE, "--command", "SELEC 1"];
        let message = said(&args, "exit status: 1", b"", recorded);
        assert!(!message.contains('\u{1b}'), "{message:?}");
        assert!(
            message.starts_with(
                "wrangler d1 execute univ --command SELEC 1 failed (exit status: 1); its standard \
                 error:\n"
            ),
            "{message}"
        );
        assert!(
            message.contains("✘ [ERROR] near \"SELEC\": syntax error at offset 0: SQLITE_ERROR"),
            "{message}"
        );
        assert!(message.contains("Logs were written to"), "{message}");

        // Only the last lines of a long standard error, and the JSON error where there is one.
        let long = (1..=30)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let json = br#"{"error": {"text": "D1 refused it"}}"#;
        let message = said(&args, "exit status: 1", json, long.as_bytes());
        assert!(
            message.contains("failed (exit status: 1): D1 refused it"),
            "{message}"
        );
        assert!(
            message.contains("its last 20 lines:\nline 11\n"),
            "{message}"
        );
        assert!(message.ends_with("line 30") && !message.contains("line 10\n"));
        assert_eq!(
            strip_ansi("\u{1b}]8;;http://x\u{7}link\u{1b}]8;;\u{1b}\\ ok"),
            "link ok"
        );
    }
}
