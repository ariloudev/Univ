//! Who a request comes from, and whether it may be answered (read-anonymously,
//! write-with-an-api-key, issue-and-revoke-keys, enforce-quotas-and-rate-limits).
//!
//! A request without an `Authorization` header is anonymous. One with a bearer key is
//! authenticated before anything else: the key's SHA-256 must name a key that stands, and a
//! key unknown or revoked is refused alike, so a refusal never says which. Owner operations,
//! under `/v1/owner/`, take the owner secret as their bearer token instead.
//!
//! Then the request is counted against its rate limit: an anonymous one by its client address,
//! a keyed one by its key at its tier's rate, read from the key's row on every request so a
//! tier changed applies from the next one.

use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use univ_model::api::keys::{RATE_WINDOW_SECONDS, RateLimitTier};

use crate::backend::{Backend, Limiter, column, first};
use crate::http::Request;
use crate::problem::{Problem, ProblemType};

/// The path prefix of the owner's operations.
pub const OWNER_PREFIX: &str = "/v1/owner/";

/// Who a request comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// No key: limited by its client address.
    Anonymous { address: String },
    /// A key that stands.
    Key { id: String, tier: RateLimitTier },
    /// The owner, by the owner secret.
    Owner,
}

/// Who `request` comes from, its key or owner secret checked.
///
/// # Errors
///
/// A 401 problem for a malformed `Authorization` header, a key that does not stand, or an
/// owner operation without the owner secret; or the database unreadable.
pub async fn authenticate<B: Backend>(request: &Request, backend: &B) -> Result<Caller, Problem> {
    let token =
        match request.header("authorization") {
            None => None,
            Some(header) => Some(bearer(header).ok_or_else(|| {
                unauthorized("The Authorization header must be `Bearer <secret>`.")
            })?),
        };
    if request.path.starts_with(OWNER_PREFIX) {
        let given = token.ok_or_else(|| unauthorized("Owner operations need the owner secret."))?;
        return match backend.owner_secret() {
            Some(secret) if same(given, &secret) => Ok(Caller::Owner),
            _ => Err(unauthorized("The owner secret is not valid.")),
        };
    }
    let Some(secret) = token else {
        return Ok(Caller::Anonymous {
            address: client_address(request),
        });
    };
    let rows = backend
        .layers(
            "SELECT id, tier FROM keys WHERE secret_sha256 = ?1 AND revoked_at IS NULL",
            &[json!(sha256_hex(secret))],
        )
        .await?;
    let row = first(rows).ok_or_else(|| unauthorized("The API key is not valid."))?;
    Ok(Caller::Key {
        id: column(&row, "id")?,
        tier: column(&row, "tier")?,
    })
}

/// Counts the request against the caller's rate limit.
///
/// # Errors
///
/// A 429 problem with `Retry-After` once the caller's limit for the window is spent, or the
/// limiter unreachable.
pub async fn limit<B: Backend>(caller: &Caller, backend: &B) -> Result<(), Problem> {
    let (limiter, client) = match caller {
        Caller::Owner => return Ok(()),
        Caller::Anonymous { address } => (Limiter::Anonymous, address.clone()),
        Caller::Key { id, tier } => (Limiter::of(*tier), id.clone()),
    };
    if backend.limit(limiter, &client).await? {
        return Ok(());
    }
    Err(Problem::new(
        ProblemType::RateLimited,
        format!(
            "This client made more than {} requests in {RATE_WINDOW_SECONDS} seconds.",
            limiter.requests()
        ),
    )
    .with("limit", limiter.requests())
    .with("window_seconds", RATE_WINDOW_SECONDS))
}

/// The SHA-256 of `secret`, as lower-case hex: what is stored of a key's secret.
#[must_use]
pub fn sha256_hex(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The token of a bearer `Authorization` header; the scheme's case is ignored (RFC 9110
/// §11.1).
fn bearer(header: &str) -> Option<&str> {
    let (scheme, token) = header.trim().split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// Whether two secrets are equal, compared in a time that does not depend on where they
/// differ.
fn same(given: &str, secret: &str) -> bool {
    Sha256::digest(given.as_bytes())
        .ct_eq(&Sha256::digest(secret.as_bytes()))
        .into()
}

/// The address the request came from, as Cloudflare names it.
fn client_address(request: &Request) -> String {
    request
        .header("cf-connecting-ip")
        .unwrap_or("unknown")
        .to_owned()
}

fn unauthorized(detail: &str) -> Problem {
    Problem::new(ProblemType::Unauthorized, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bearer_token_is_read_from_the_authorization_header() {
        assert_eq!(bearer("Bearer abc"), Some("abc"));
        assert_eq!(bearer("bearer  abc "), Some("abc"));
        assert_eq!(bearer("Basic abc"), None);
        assert_eq!(bearer("Bearer"), None);
        assert_eq!(bearer("Bearer "), None);
    }

    #[test]
    fn the_sha256_is_lower_case_hex() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
