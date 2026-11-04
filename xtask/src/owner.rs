//! Runs on the real local state, where `univ-ingest publish` puts releases: `cargo xtask
//! explore` and the benches. Each starts Univ Server there, issues itself an API key as the
//! owner, runs, and stops the server.
//!
//! The owner secret of the real state is the owner's own, in `crates/univ-server/.dev.vars`,
//! which git ignores. Wrangler reads it from there, so that it never appears in a process's
//! arguments, and xtask reads it from the same file, by the same rules (dotenv 16, which
//! Wrangler bundles), to issue and revoke the run's keys. The fixed test secret is the fixture
//! state's alone.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use crate::integration::{Exchange, WORKER_DIR, prepared_real_state, send, start_ready};

/// The file the Worker's local secrets are kept in, from the workspace root.
const DEV_VARS: &str = ".dev.vars";

/// The owner secret of the real state, read from the Worker's `.dev.vars`; none, having said
/// why. Read before anything slow, so that a run without it stops at once.
pub(crate) fn real_owner(root: &Path) -> Option<String> {
    let file = root.join(WORKER_DIR).join(DEV_VARS);
    let read = match fs::read_to_string(&file) {
        Ok(text) => secret_in(&text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("could not read {}: {error}", file.display());
            return None;
        }
    };
    if read.is_none() {
        eprintln!(
            "{} holds no OWNER_SECRET: create it with a line `OWNER_SECRET=<a long random \
             secret>` (git ignores the file), for the Worker on the real state to take owner \
             requests",
            file.display()
        );
    }
    read
}

/// The value of `OWNER_SECRET` in the text of a `.dev.vars` file, by dotenv 16's rules, as
/// Wrangler reads it: an optional `export `, `=` or `: ` after the name, a value in single,
/// double or back quotes or else up to a `#`, trimmed, `\n` and `\r` unescaped in double
/// quotes, and the last line of the name winning; none where it is missing or empty. A quoted
/// value over several lines is not read.
fn secret_in(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let line = line
                .strip_prefix("export")
                .filter(|rest| rest.starts_with(char::is_whitespace))
                .map_or(line, str::trim_start);
            let rest = line.strip_prefix("OWNER_SECRET")?;
            let value = match rest.trim_start().strip_prefix('=') {
                Some(value) => value,
                None => rest
                    .strip_prefix(':')
                    .filter(|v| v.starts_with(char::is_whitespace))?,
            };
            Some(unquoted(value.trim_start()))
        })
        .next_back()
        .filter(|value| !value.is_empty())
}

/// A dotenv value as written after its name: its quotes removed, or its comment.
fn unquoted(value: &str) -> String {
    let quoted = ['"', '\'', '`'].into_iter().find_map(|quote| {
        let inner = value.strip_prefix(quote)?;
        let end = inner.find(quote)?;
        let after = inner[end + 1..].trim_start();
        (after.is_empty() || after.starts_with('#')).then(|| (quote, &inner[..end]))
    });
    match quoted {
        Some(('"', inner)) => inner.replace("\\n", "\n").replace("\\r", "\r"),
        Some((_, inner)) => inner.to_owned(),
        None => value
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    }
}

/// Runs `run` with the port of Univ Server on the real local state and a fresh key labelled
/// `label`, issued with `owner`, its Wrangler log at `log`, then stops the server; whether all
/// of it succeeded.
pub(crate) fn with_real_server(
    root: &Path,
    owner: &str,
    log: &Path,
    label: &str,
    run: impl FnOnce(u16, &str) -> bool,
) -> bool {
    let Some(state) = prepared_real_state(root) else {
        return false;
    };
    if let Some(dir) = log.parent()
        && let Err(error) = fs::create_dir_all(dir)
    {
        eprintln!("could not create {}: {error}", dir.display());
        return false;
    }
    // No secret given: Wrangler reads the owner's from `.dev.vars`.
    let Some((wrangler, port)) = start_ready(root, &state, log, None) else {
        eprintln!(
            "failed at: starting Univ Server; its log is {}",
            log.display()
        );
        return false;
    };
    let Some(key) = fresh_key(port, owner, label) else {
        return false;
    };
    let succeeded = run(port, &key);
    drop(wrangler);
    succeeded
}

/// A key of the extended tier labelled `label`, issued with the owner secret `owner` by the
/// Worker on `port`, where anonymous reads are limited to 300 a minute; the keys of `label`
/// that earlier runs left standing are revoked first, as Ctrl-C ends a run before it can revoke
/// its own.
pub(crate) fn fresh_key(port: u16, owner: &str, label: &str) -> Option<String> {
    let key = revoke_labelled(port, owner, label)
        .and_then(|()| issue_key(port, owner, "extended", label));
    key.map_err(|error| eprintln!("could not issue a {label} key: {error}"))
        .ok()
}

/// Sends an owner request to the Worker on `port`, and gives its status and JSON document.
fn as_owner(
    port: u16,
    owner: &str,
    method: &str,
    path: &str,
    json: Option<&str>,
) -> Result<(u16, serde_json::Value), String> {
    let exchange = Exchange {
        method,
        path,
        bearer: Some(owner),
        json,
    };
    let (status, body) = send(port, &exchange, Duration::from_secs(30))?;
    if status == 401 {
        return Err(format!(
            "the Worker refused the owner secret ({method} {path} answered 401): the Worker and \
             xtask read different values of OWNER_SECRET in {WORKER_DIR}/{DEV_VARS}"
        ));
    }
    let document = body
        .find('{')
        .zip(body.rfind('}'))
        .map_or("", |(start, end)| &body[start..=end]);
    match serde_json::from_str(document) {
        Ok(value) => Ok((status, value)),
        Err(_) if document.is_empty() => Ok((status, serde_json::Value::Null)),
        Err(error) => Err(format!("{method} {path} answered {status}: {error}")),
    }
}

/// Issues a key of `tier` labelled `label` from the Worker on `port`, and gives its secret.
fn issue_key(port: u16, owner: &str, tier: &str, label: &str) -> Result<String, String> {
    let json = format!(r#"{{"label":"{label}","tier":"{tier}"}}"#);
    let (status, issued) = as_owner(port, owner, "POST", "/v1/owner/keys", Some(&json))?;
    issued["secret"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("issuing a key answered {status}: {issued}"))
}

/// Revokes every standing key labelled `label` on the Worker at `port`: what an earlier run
/// issued for itself and could not revoke, as Ctrl-C ends it at once.
fn revoke_labelled(port: u16, owner: &str, label: &str) -> Result<(), String> {
    let (status, keys) = as_owner(port, owner, "GET", "/v1/owner/keys", None)?;
    if status != 200 {
        return Err(format!("listing the keys answered {status}: {keys}"));
    }
    let standing = keys["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|key| key["label"] == label && key["revoked_at"].is_null())
        .filter_map(|key| key["id"].as_str());
    for id in standing {
        let path = format!("/v1/owner/keys/{id}/revoke");
        let (status, _) = as_owner(port, owner, "POST", &path, None)?;
        if status != 200 {
            return Err(format!("revoking key {id} answered {status}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::secret_in;

    #[test]
    fn the_owner_secret_is_read_as_wranglers_dotenv_reads_it() {
        let read = |text: &str| secret_in(text);
        assert_eq!(read("OWNER_SECRET=abc123\n").as_deref(), Some("abc123"));
        assert_eq!(
            read("A=1\nOWNER_SECRET = \"s e\"\n").as_deref(),
            Some("s e")
        );
        assert_eq!(read("OWNER_SECRET='x' # mine").as_deref(), Some("x"));
        assert_eq!(read("OWNER_SECRET=`x`").as_deref(), Some("x"));
        assert_eq!(read("export OWNER_SECRET=e").as_deref(), Some("e"));
        assert_eq!(read("OWNER_SECRET: c").as_deref(), Some("c"));
        // Unquoted, a value ends at a `#`; the last line of the name wins.
        assert_eq!(read("OWNER_SECRET=Ab3#xYz").as_deref(), Some("Ab3"));
        assert_eq!(
            read("OWNER_SECRET=old\nOWNER_SECRET=new").as_deref(),
            Some("new")
        );
        assert_eq!(read("OWNER_SECRET=\"a\\nb\"").as_deref(), Some("a\nb"));
    }

    #[test]
    fn a_missing_or_empty_owner_secret_is_none() {
        assert_eq!(secret_in(""), None);
        assert_eq!(secret_in("OWNER_SECRET=\n"), None);
        assert_eq!(
            secret_in("OWNER_SECRETS=x\n# OWNER_SECRET=y\nexportOWNER_SECRET=z"),
            None
        );
    }
}
