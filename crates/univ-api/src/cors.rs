//! Cross-origin reads from any origin (read-anonymously).
//!
//! Every response allows any origin. A preflight is answered for any path without
//! credentials, since nothing Univ Server serves depends on the caller's cookies: a key travels
//! in the `Authorization` header, which the preflight lists by name (a wildcard would not admit
//! it).

use crate::http::Response;

/// The methods a preflight admits.
const METHODS: &str = "GET, HEAD, POST, PATCH, DELETE, OPTIONS";

/// The request headers a preflight admits.
const REQUEST_HEADERS: &str = "Authorization, Content-Type, If-None-Match";

/// The response headers a cross-origin script may read beyond the safelisted ones.
const EXPOSED_HEADERS: &str = "ETag, Location, Retry-After, Univ-Release";

/// How long a browser may keep a preflight's answer, in seconds.
const MAX_AGE: &str = "86400";

/// The answer to a CORS preflight: 204 with what may follow.
pub fn preflight() -> Response {
    Response::empty(204)
        .with_header("access-control-allow-methods", METHODS)
        .with_header("access-control-allow-headers", REQUEST_HEADERS)
        .with_header("access-control-max-age", MAX_AGE)
}

/// The response, allowed to any origin and with its readable headers exposed.
pub fn allow_any_origin(response: Response) -> Response {
    response
        .with_header("access-control-allow-origin", "*")
        .with_header("access-control-expose-headers", EXPOSED_HEADERS)
}
