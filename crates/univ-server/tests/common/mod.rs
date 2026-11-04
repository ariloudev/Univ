//! What every end-to-end test file shares: the Worker's address and owner secret, as
//! `cargo xtask integration` gives them, and one HTTP client.
#![allow(dead_code, reason = "each test file uses its own part of this module")]

use std::sync::LazyLock;

use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{Value, json};

/// The Worker's base URL.
pub static BASE: LazyLock<String> = LazyLock::new(|| {
    std::env::var("UNIV_E2E_URL")
        .expect("UNIV_E2E_URL names the Worker: run `cargo xtask integration`")
});

/// The Worker's owner secret.
pub static OWNER: LazyLock<String> = LazyLock::new(|| {
    std::env::var("UNIV_E2E_OWNER_SECRET")
        .expect("UNIV_E2E_OWNER_SECRET is the Worker's: run `cargo xtask integration`")
});

/// One client for every test, which follows no redirect so that a test sees it.
pub static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
});

pub fn url(path: &str) -> String {
    format!("{}{path}", *BASE)
}

pub fn get(path: &str) -> Response {
    CLIENT.get(url(path)).send().unwrap()
}

pub fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response
        .headers()
        .get(name)
        .map(|value| value.to_str().unwrap())
}

pub fn json(response: Response) -> Value {
    serde_json::from_slice(&response.bytes().unwrap()).unwrap()
}

/// `request` sent with the owner secret.
pub fn owner(request: RequestBuilder) -> RequestBuilder {
    request.bearer_auth(&*OWNER)
}

/// `request` with `body` as its JSON body.
pub fn with_json(request: RequestBuilder, body: &Value) -> RequestBuilder {
    request
        .header("content-type", "application/json")
        .body(body.to_string())
}

/// Issues a key of `tier`, and gives its id and secret.
pub fn issue(label: &str, tier: &str) -> (String, String) {
    let request = owner(CLIENT.post(url("/v1/owner/keys")));
    let response = with_json(request, &json!({"label": label, "tier": tier}))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let issued = json(response);
    (
        issued["id"].as_str().unwrap().to_owned(),
        issued["secret"].as_str().unwrap().to_owned(),
    )
}
