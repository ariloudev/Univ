//! A request that fails names why: the error's causes, such as a refused connection, a reply
//! that is not HTTP, a timeout, a TLS or a DNS failure, follow its own message in the error a
//! run shows, not only "error sending request for url". Local mock servers only.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

use common::{accept, item, keep, rig};
use univ_ingest::fetch::{FetchError, with_causes};

/// The error of fetching one chunk from the single mirror `mirror`.
async fn failure(mirror: String) -> String {
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("chunk-0.tsv");
    let error = rig
        .fetcher
        .chunk(&item("chunk 0", &dest), &[mirror], "/q", &accept, &keep)
        .await
        .unwrap_err();
    let FetchError::AllMirrorsFailed { .. } = &error else {
        unreachable!("{error}")
    };
    error.to_string()
}

#[tokio::test]
async fn a_refused_connection_is_named_after_the_request_error() {
    // A port just let go of: nothing listens there.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let shown = failure(format!("http://127.0.0.1:{port}")).await;
    assert!(shown.contains("error sending request"), "{shown}");
    assert!(
        shown.to_lowercase().contains("connection refused"),
        "{shown}"
    );
}

#[tokio::test]
async fn a_reply_that_is_not_http_is_named_after_the_request_error() {
    // A mock server that reads each request and answers with bytes that are not HTTP.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten().take(5) {
            let mut stream = stream;
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"SSH-2.0-not-http\r\n\r\n");
        }
    });
    let shown = failure(format!("http://127.0.0.1:{port}")).await;
    assert!(shown.contains("error sending request"), "{shown}");
    assert!(
        shown.to_lowercase().contains("invalid http version"),
        "the cause hyper gives: {shown}"
    );
}

#[derive(Debug)]
struct Layer(&'static str, Option<Box<Layer>>);

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Layer {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.1.as_deref().map(|l| l as _)
    }
}

#[test]
fn each_cause_follows_once_and_a_repeated_one_is_left_out() {
    let timeout = Layer(
        "error sending request for url (https://example.org/q)",
        Some(Box::new(Layer(
            "operation timed out",
            Some(Box::new(Layer("timed out", None))),
        ))),
    );
    assert_eq!(
        with_causes(&timeout),
        "error sending request for url (https://example.org/q): operation timed out"
    );
    let dns = Layer(
        "error sending request",
        Some(Box::new(Layer(
            "client error (Connect)",
            Some(Box::new(Layer("dns error: failed to lookup address", None))),
        ))),
    );
    assert_eq!(
        with_causes(&dns),
        "error sending request: client error (Connect): dns error: failed to lookup address"
    );
}
