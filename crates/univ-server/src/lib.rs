//! The Cloudflare Worker of Univ Server: it turns each runtime request into a `univ_api`
//! request, lets [`univ_api::handle`] answer it, and turns the answer back.
//!
//! Its bindings are named in `wrangler.toml` beside this crate. Nothing here decides what a
//! request means; that is all `univ_api`'s. A failure of the runtime itself is still answered
//! as a problem with the CORS headers, so a browser on another origin can read it.

mod d1;

use univ_api::{Method, ProblemType};
use worker::{Context, Env, Error, Headers, Request, Response, Result, event};

#[event(fetch)]
async fn fetch(request: Request, env: Env, _ctx: Context) -> Result<Response> {
    let head = request.method() == worker::Method::Head;
    let answer = match answer(request, &env).await {
        Ok(answer) => answer,
        Err(error) => univ_api::failure(
            ProblemType::InternalError,
            format!("The Worker failed to answer: {error}."),
        ),
    };
    match respond(answer, &env, head).await {
        Ok(response) => Ok(response),
        Err(error) => {
            respond(
                univ_api::failure(
                    ProblemType::InternalError,
                    format!("The Worker failed to send its answer: {error}."),
                ),
                &env,
                head,
            )
            .await
        }
    }
}

/// `univ_api`'s answer to the runtime request.
async fn answer(mut request: Request, env: &Env) -> Result<univ_api::Response> {
    let url = request.url()?;
    let method = Method::parse(request.method().as_ref());
    // Only a write carries a body; a read's is never read, whatever it sends.
    let body = if matches!(method, Method::Post | Method::Patch | Method::Delete) {
        match request.bytes().await {
            Ok(body) => body,
            Err(error) => {
                return Ok(univ_api::failure(
                    ProblemType::UnreadableBody,
                    format!("The request's body could not be read: {error}."),
                ));
            }
        }
    } else {
        Vec::new()
    };
    let api_request = univ_api::Request {
        method,
        path: url.path().to_owned(),
        query: url.query().unwrap_or_default().to_owned(),
        headers: request.headers().entries().collect(),
        body,
    };
    let backend = d1::D1::new(env)?;
    Ok(univ_api::handle(&api_request, &backend).await)
}

/// The runtime response that carries `answer`. An object of the releases bucket is handed to
/// the runtime as a stream, which it sends without the Worker reading a byte of it; to a HEAD
/// it answers with the object's size, read from its metadata alone.
async fn respond(answer: univ_api::Response, env: &Env, head: bool) -> Result<Response> {
    let headers = Headers::new();
    for (name, value) in &answer.headers {
        headers.append(name, value)?;
    }
    let response = match answer.body {
        univ_api::Body::Empty => Response::empty()?,
        univ_api::Body::Bytes(bytes) => Response::from_bytes(bytes)?,
        univ_api::Body::Static(bytes) => Response::from_bytes(bytes.to_vec())?,
        univ_api::Body::Object(key) if head => {
            let object = env
                .bucket(d1::BUCKET)?
                .head(key.as_str())
                .await?
                .ok_or_else(|| Error::RustError(format!("the releases bucket holds no {key}")))?;
            headers.set("content-length", &object.size().to_string())?;
            Response::empty()?
        }
        univ_api::Body::Object(key) => {
            let object = env
                .bucket(d1::BUCKET)?
                .get(key.as_str())
                .execute()
                .await?
                .ok_or_else(|| Error::RustError(format!("the releases bucket holds no {key}")))?;
            let body = object
                .body()
                .ok_or_else(|| Error::RustError(format!("{key} has no body")))?;
            Response::from_body(body.response_body()?)?
        }
    };
    Ok(response.with_status(answer.status).with_headers(headers))
}
