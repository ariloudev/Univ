//! What every end-to-end test file shares: the Worker's address and owner secret, as
//! `cargo xtask integration` gives them, and one HTTP client.
#![allow(dead_code, reason = "each test file uses its own part of this module")]

use std::sync::LazyLock;

use reqwest::blocking::{Client, Response};
use serde_json::Value;

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
