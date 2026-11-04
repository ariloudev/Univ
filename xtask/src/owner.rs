//! Runs on the real local state, where `univ-ingest publish` puts releases: `cargo xtask
//! explore` and the benches. Each starts Univ Server there, issues itself an API key as the
//! owner, runs, and stops the server.
//!
//! The owner secret of the real state is the owner's own, in `crates/univ-server/.dev.vars`,
//! which git ignores: Wrangler reads it from there, and xtask reads it from the same file to
//! issue and revoke the run's keys. The fixed test secret is the fixture state's alone.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::integration::{
    Exchange, REAL_STATE, WORKER_DIR, Wrangler, migrate, release_tables, send, start_ready,
};

/// Runs `run` with the port of Univ Server on the real local state and a fresh key labelled
/// `label`, its Wrangler log at `log`, then stops the server; whether all of it succeeded.
pub(crate) fn with_real_server(
    root: &Path,
    log: &Path,
    label: &str,
    run: impl FnOnce(u16, &str) -> bool,
) -> bool {
    // Read first: without it the Worker would refuse every owner request after its long build.
    let owner = match dev_vars_secret(root) {
        Ok(secret) => secret,
        Err(error) => {
            eprintln!("{error}");
            return false;
        }
    };
    let Some((wrangler, port)) = start_on_real_state(root, log) else {
        eprintln!(
            "failed at: starting Univ Server; its log is {}",
            log.display()
        );
        return false;
    };
    let Some(key) = fresh_key(port, &owner, label) else {
        return false;
    };
    let succeeded = run(port, &key);
    drop(wrangler);
    succeeded
}

/// The owner secret in the Worker's `.dev.vars`, or why there is none.
fn dev_vars_secret(root: &Path) -> Result<String, String> {
    let file = root.join(WORKER_DIR).join(".dev.vars");
    let missing = || {
        format!(
            "{} holds no OWNER_SECRET: create it with a line `OWNER_SECRET=<a long random \
             secret>` (git ignores the file), for the Worker on the real state to take owner \
             requests",
            file.display()
        )
    };
    let text = fs::read_to_string(&file).map_err(|_| missing())?;
    secret_in(&text).ok_or_else(missing)
}

/// The value of `OWNER_SECRET` in the text of a `.dev.vars` file: dotenv lines, the value
/// unquoted; none where it is missing or empty.
fn secret_in(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("OWNER_SECRET"))
        .find_map(|rest| rest.trim_start().strip_prefix('='))
        .map(|value| {
            let value = value.trim();
            ['"', '\'']
                .iter()
                .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
                .unwrap_or(value)
                .to_owned()
        })
        .filter(|value| !value.is_empty())
}

/// Univ Server on the real local state, its tables and migrations brought up to date, its
/// Wrangler log at `log`, and its owner secret the one Wrangler reads from `.dev.vars`; the
/// running Wrangler and its port, or none, having said why.
fn start_on_real_state(root: &Path, log: &Path) -> Option<(Wrangler, u16)> {
    let state = root.join(REAL_STATE);
    if !state.is_dir() {
        eprintln!(
            "{} holds no published release: run `univ-ingest publish` first",
            state.display()
        );
        return None;
    }
    if !release_tables(root, &state) || !migrate(root, &state) {
        return None;
    }
    if let Some(dir) = log.parent()
        && let Err(error) = fs::create_dir_all(dir)
    {
        eprintln!("could not create {}: {error}", dir.display());
        return None;
    }
    let started = start_ready(root, &state, log, None);
    if started.is_none() {
        eprintln!("wrangler dev did not start; its log is {}", log.display());
    }
    started
}

/// A key of the extended tier labelled `label`, issued with the owner secret `owner` by the
/// Worker on `port`, where anonymous reads are limited to 300 a minute; the keys of `label`
/// that earlier runs left standing are revoked first, as Ctrl-C ends a run before it can revoke
/// its own.
pub(crate) fn fresh_key(port: u16, owner: &str, label: &str) -> Option<String> {
    let key =
        revoke_labelled(port, owner, label).and_then(|_| issue_key(port, owner, "extended", label));
    key.map_err(|error| eprintln!("could not issue a {label} key: {error}"))
        .ok()
}

/// A request to the owner routes, with the owner secret.
fn as_owner<'a>(
    method: &'a str,
    path: &'a str,
    owner: &'a str,
    json: Option<&'a str>,
) -> Exchange<'a> {
    Exchange {
        method,
        path,
        bearer: Some(owner),
        json,
    }
}

/// Issues a key of `tier` labelled `label` from the Worker on `port`, and gives its secret.
fn issue_key(port: u16, owner: &str, tier: &str, label: &str) -> Result<String, String> {
    let json = format!(r#"{{"label":"{label}","tier":"{tier}"}}"#);
    let exchange = as_owner("POST", "/v1/owner/keys", owner, Some(&json));
    let (status, body) = send(port, &exchange, Duration::from_secs(30))?;
    let issued: serde_json::Value = serde_json::from_str(document(&body))
        .map_err(|error| format!("issuing a key answered {status}: {error}"))?;
    issued["secret"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("issuing a key answered {status}: {issued}"))
}

/// The JSON document of a Worker's answer, which may come in one chunk of chunked transfer
/// coding: the document is its outermost braces.
fn document(body: &str) -> &str {
    body.find('{')
        .zip(body.rfind('}'))
        .map_or("", |(start, end)| &body[start..=end])
}

/// Revokes every standing key labelled `label` on the Worker at `port`: what an earlier run
/// issued for itself and could not revoke, as Ctrl-C ends it at once.
fn revoke_labelled(port: u16, owner: &str, label: &str) -> Result<usize, String> {
    let list = as_owner("GET", "/v1/owner/keys", owner, None);
    let (status, body) = send(port, &list, Duration::from_secs(30))?;
    let keys: serde_json::Value = serde_json::from_str(document(&body))
        .map_err(|error| format!("listing the keys answered {status}: {error}"))?;
    let standing: Vec<&str> = keys["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|key| key["label"] == label && key["revoked_at"].is_null())
        .filter_map(|key| key["id"].as_str())
        .collect();
    for id in &standing {
        let path = format!("/v1/owner/keys/{id}/revoke");
        let (status, _) = send(
            port,
            &as_owner("POST", &path, owner, None),
            Duration::from_secs(30),
        )?;
        if status != 200 {
            return Err(format!("revoking key {id} answered {status}"));
        }
    }
    Ok(standing.len())
}

#[cfg(test)]
mod tests {
    use super::secret_in;

    #[test]
    fn the_owner_secret_is_read_from_dev_vars_plain_or_quoted() {
        assert_eq!(
            secret_in("OWNER_SECRET=abc123\n").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            secret_in("A=1\nOWNER_SECRET = \"s e\"\n").as_deref(),
            Some("s e")
        );
        assert_eq!(secret_in("OWNER_SECRET='x'").as_deref(), Some("x"));
    }

    #[test]
    fn a_missing_or_empty_owner_secret_is_none() {
        assert_eq!(secret_in(""), None);
        assert_eq!(secret_in("OWNER_SECRET=\n"), None);
        assert_eq!(secret_in("OWNER_SECRETS=x\n# OWNER_SECRET=y"), None);
    }
}
