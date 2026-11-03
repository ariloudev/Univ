//! Every R2 operation of one run through one long-running process: `scripts/r2-batch.mjs`, run
//! with the Node that runs Wrangler. It opens the local bucket that `wrangler r2 object …
//! --local --persist-to` opens, the way Wrangler does, so what it writes Wrangler and
//! `wrangler dev` read, and the reverse.
//!
//! Each operation is one JSON line on the script's standard input, and the script answers one
//! JSON line per object on its standard output, in order; the script's header gives the
//! protocol. A put is read back by the script, which gives the size and SHA-256 of what R2 gave
//! back; a delete of many keys is sent as one operation, which R2 deletes 1,000 keys at a time.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Deserialize;
use serde_json::json;

use super::ReadBack;
use super::wrangler::{BUCKET, STDERR_LINES, strip_ansi};

/// The script, from the directory of the Wrangler configuration.
pub const SCRIPT: &str = "scripts/r2-batch.mjs";

/// The lines of standard error a process wrote last, at most [`STDERR_LINES`].
type Tail = Arc<Mutex<VecDeque<String>>>;

/// The batch script, running.
#[derive(Debug)]
pub struct R2Batch {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr: Tail,
    drain: Option<JoinHandle<()>>,
}

/// One answer of the script, for one object.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    #[serde(default)]
    pub key: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// The answer `line` of the script about the object at `key` in operation `op`: an error naming
/// the object and the cause when R2 refused it or the line is not an answer about `key`.
///
/// # Errors
///
/// Fails, naming `key`, when the line says R2 failed or is not an answer about `key`.
pub fn answer(line: &str, op: &str, key: &str) -> Result<Answer, String> {
    let answer: Answer = serde_json::from_str(line.trim()).map_err(|e| {
        format!("R2 {op} of {key}: the batch script answered {line:?}, not an answer ({e})")
    })?;
    if answer.key.as_deref() != Some(key) {
        return Err(format!(
            "R2 {op} of {key}: the batch script answered about {:?} instead{}",
            answer.key,
            answer
                .error
                .as_deref()
                .map_or_else(String::new, |e| format!(": {e}"))
        ));
    }
    if !answer.ok {
        let cause = answer.error.as_deref().unwrap_or("no cause given");
        return Err(format!("R2 {op} of {key} failed: {cause}"));
    }
    Ok(answer)
}

impl R2Batch {
    /// Starts the script of the repository `root`, the Wrangler configuration's directory, on
    /// the local state directory `persist_to`, with the Node on the search path that runs
    /// Wrangler from `node_modules/.bin`.
    ///
    /// # Errors
    ///
    /// Fails, saying why, when the script is missing or Node cannot be run.
    pub fn start(root: &Path, persist_to: &Path) -> Result<Self, String> {
        let script = root.join(SCRIPT);
        if !script.is_file() {
            return Err(format!("{} does not exist", script.display()));
        }
        let mut child = Command::new("node")
            .current_dir(root)
            .arg(&script)
            .arg("--persist-to")
            .arg(persist_to)
            .arg("--bucket")
            .arg(BUCKET)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "node {}: {e}; the Node that runs Wrangler must be on the search path",
                    script.display()
                )
            })?;
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
            let _ = child.kill();
            return Err("the batch script's output could not be read".to_owned());
        };
        let tail: Tail = Arc::default();
        let kept = Arc::clone(&tail);
        let drain = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Ok(mut lines) = kept.lock() {
                    lines.push_back(strip_ansi(&line));
                    while lines.len() > STDERR_LINES {
                        lines.pop_front();
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: tail,
            drain: Some(drain),
        })
    }

    /// Why the script stopped answering about `key`: whether it ended, and its last lines of
    /// standard error.
    fn stopped(&mut self, op: &str, key: &str, why: &str) -> String {
        let status = match self.child.try_wait() {
            Ok(Some(status)) => format!("it ended ({status})"),
            _ => "it is still running".to_owned(),
        };
        let lines: Vec<String> = self
            .stderr
            .lock()
            .map(|l| l.iter().filter(|l| !l.trim().is_empty()).cloned().collect())
            .unwrap_or_default();
        let said = if lines.is_empty() {
            "it wrote nothing to standard error".to_owned()
        } else {
            format!("its standard error:\n{}", lines.join("\n"))
        };
        format!("R2 {op} of {key}: the batch script {why}; {status}; {said}")
    }

    /// Sends the operation `line`, about `key`.
    fn send(&mut self, op: &str, key: &str, line: &serde_json::Value) -> Result<(), String> {
        let sent = match self.stdin.as_mut() {
            Some(stdin) => writeln!(stdin, "{line}").and_then(|()| stdin.flush()),
            None => Err(std::io::Error::other("its input is closed")),
        };
        sent.map_err(|e| self.stopped(op, key, &format!("took no operation ({e})")))
    }

    /// Reads the answer about `key`.
    fn receive(&mut self, op: &str, key: &str) -> Result<Answer, String> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => Err(self.stopped(op, key, "gave no answer")),
            Ok(_) => answer(&line, op, key),
            Err(e) => Err(self.stopped(op, key, &format!("could not be read ({e})"))),
        }
    }

    /// Writes `file` at `key` with the MIME type `content_type`, and reads it back: the size and
    /// SHA-256 of what R2 gave back, or none when R2 held nothing at `key`.
    ///
    /// # Errors
    ///
    /// Fails, naming the object and the cause, when R2 refuses the write or the read.
    pub fn put(
        &mut self,
        key: &str,
        file: &Path,
        content_type: &str,
    ) -> Result<Option<ReadBack>, String> {
        let op = json!({"op": "put", "key": key, "file": file, "contentType": content_type});
        self.send("put", key, &op)?;
        let back = self.receive("put", key)?;
        if !back.found {
            return Ok(None);
        }
        match (back.bytes, back.sha256) {
            (Some(bytes), Some(sha256)) => Ok(Some(ReadBack { bytes, sha256 })),
            _ => Err(format!(
                "R2 put of {key}: the batch script gave no size and SHA-256 of what it read back"
            )),
        }
    }

    /// Writes the object at `key` to `file`: whether R2 held one.
    ///
    /// # Errors
    ///
    /// Fails, naming the object and the cause, when R2 cannot be read.
    pub fn get(&mut self, key: &str, file: &Path) -> Result<bool, String> {
        self.send("get", key, &json!({"op": "get", "key": key, "file": file}))?;
        Ok(self.receive("get", key)?.found)
    }

    /// Deletes the objects at `keys`, those there are, in one operation.
    ///
    /// # Errors
    ///
    /// Fails, naming the first object R2 did not delete and the cause.
    pub fn delete(&mut self, keys: &[String]) -> Result<(), String> {
        let Some(first) = keys.first() else {
            return Ok(());
        };
        self.send("delete", first, &json!({"op": "delete", "keys": keys}))?;
        // Every answer is read, so the next operation reads its own.
        let mut failed = None;
        for key in keys {
            if let Err(e) = self.receive("delete", key) {
                failed.get_or_insert(e);
            }
        }
        failed.map_or(Ok(()), Err)
    }
}

impl Drop for R2Batch {
    /// Closes the script's input, which ends it once it has closed the bucket.
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
        if let Some(drain) = self.drain.take() {
            let _ = drain.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_names_the_object_and_the_cause_of_a_failure() {
        let put = answer(
            r#"{"key":"releases/1/a.bin","ok":true,"found":true,"bytes":3,"sha256":"ab"}"#,
            "put",
            "releases/1/a.bin",
        )
        .unwrap();
        assert_eq!((put.bytes, put.sha256.as_deref()), (Some(3), Some("ab")));
        let missing = answer(r#"{"key":"k","ok":true,"found":false}"#, "get", "k").unwrap();
        assert!(!missing.found);

        let refused = answer(
            r#"{"key":"releases/1/a.bin","ok":false,"error":"disk full"}"#,
            "delete",
            "releases/1/a.bin",
        )
        .unwrap_err();
        assert_eq!(refused, "R2 delete of releases/1/a.bin failed: disk full");
        let other = answer(r#"{"key":"b","ok":true}"#, "delete", "a").unwrap_err();
        assert!(other.contains("about Some(\"b\") instead"), "{other}");
        let unread = answer(r#"{"ok":false,"error":"no operation"}"#, "get", "a").unwrap_err();
        assert!(unread.ends_with(": no operation"), "{unread}");
        let noise = answer("workerd says hello", "put", "a").unwrap_err();
        assert!(
            noise.starts_with("R2 put of a: the batch script answered"),
            "{noise}"
        );
    }
}
