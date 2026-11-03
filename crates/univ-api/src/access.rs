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
//! tier changed applies from the next one. A request whose credentials fail counts as
//! anonymous, so credentials cannot be guessed faster than the address's limit.

use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use univ_model::api::keys::{RATE_WINDOW_SECONDS, RateLimitTier};
use univ_model::encoding::hex;

use crate::backend::{Backend, Limiter, column, first};
use crate::http::Request;
use crate::problem::{Problem, ProblemType};

/// The path prefix of the owner's operations.
pub const OWNER_PREFIX: &str = "/v1/owner/";

/// Who a request comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// No key: limited by its client address.
    Anonymous,
    /// A key that stands.
    Key { id: String, tier: RateLimitTier },
    /// The owner, by the owner secret.
    Owner,
}

/// Authenticates `request`, then counts it against its caller's rate limit.
///
/// # Errors
///
/// A 401 for credentials that fail, a 429 once the caller's limit is spent (credentials that
/// fail count against the address's), or the database or limiter failing.
pub async fn admit<B: Backend>(request: &Request, backend: &B) -> Result<Caller, Problem> {
    let caller = match authenticate(request, backend).await {
        Ok(caller) => caller,
        Err(problem) if problem.kind == ProblemType::Unauthorized => {
            limit(request, &Caller::Anonymous, backend).await?;
            return Err(problem);
        }
        Err(problem) => return Err(problem),
    };
    limit(request, &caller, backend).await?;
    Ok(caller)
}

/// Who `request` comes from, its key or owner secret checked.
async fn authenticate<B: Backend>(request: &Request, backend: &B) -> Result<Caller, Problem> {
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
        return Ok(Caller::Anonymous);
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
async fn limit<B: Backend>(request: &Request, caller: &Caller, backend: &B) -> Result<(), Problem> {
    let (limiter, client) = match caller {
        Caller::Owner => return Ok(()),
        Caller::Anonymous => (Limiter::Anonymous, client_address(request)),
        Caller::Key { id, tier } => (Limiter::Key(*tier), id.as_str()),
    };
    if backend.limit(limiter, client).await? {
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
    .with("window_seconds", RATE_WINDOW_SECONDS)
    // RFC 6585 §4: when the client may try again.
    .with_header("retry-after", &RATE_WINDOW_SECONDS.to_string()))
}

/// The SHA-256 of `secret`, as lower-case hex: what is stored of a key's secret.
#[must_use]
pub fn sha256_hex(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
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
fn client_address(request: &Request) -> &str {
    request.header("cf-connecting-ip").unwrap_or("unknown")
}

/// A 401 naming the scheme the request must use (RFC 9110 §11.6.1).
fn unauthorized(detail: &str) -> Problem {
    Problem::new(ProblemType::Unauthorized, detail).with_header("www-authenticate", "Bearer")
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
