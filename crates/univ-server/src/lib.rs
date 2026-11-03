//! The Cloudflare Worker of Univ Server: it turns each runtime request into a `univ_api`
//! request, lets [`univ_api::handle`] answer it, and turns the answer back.
//!
//! Its bindings are named in `wrangler.toml` beside this crate. Nothing here decides what a
//! request means; that is all `univ_api`'s.

use worker::{Context, Env, Headers, Request, Response, Result, event};

#[event(fetch)]
async fn fetch(mut request: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let url = request.url()?;
    let mut headers = Vec::new();
    for (name, value) in request.headers() {
        headers.push((name.to_ascii_lowercase(), value));
    }
    let body = request.bytes().await.unwrap_or_default();
    let api_request = univ_api::Request {
        method: univ_api::Method::parse(request.method().as_ref()),
        path: url.path().to_owned(),
        query: url.query().unwrap_or_default().to_owned(),
        headers,
        body,
    };
    respond(univ_api::handle(&api_request))
}

/// The runtime response that carries `answer`.
fn respond(answer: univ_api::Response) -> Result<Response> {
    let headers = Headers::new();
    for (name, value) in &answer.headers {
        headers.append(name, value)?;
    }
    let response = match answer.body {
        univ_api::Body::Empty => Response::empty()?,
        univ_api::Body::Bytes(bytes) => Response::from_bytes(bytes)?,
    };
    Ok(response.with_status(answer.status).with_headers(headers))
}
