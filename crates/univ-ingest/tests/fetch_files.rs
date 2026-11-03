//! A single file resumes by byte range, starts over when it must, and is checked once whole (F-2).
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use std::path::Path;

use common::{rig, source};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use univ_ingest::events::{Event, RestartReason};
use univ_ingest::fetch::{Expected, FetchError, Fetched};
use wiremock::matchers::{header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The 100 bytes of the file.
fn body() -> Vec<u8> {
    (0u8..100).collect()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Leaves the first 40 bytes of `url` in `dest`'s partial file, as a stopped run would.
fn stopped_at_40(dest: &Path, url: &str) {
    let part = dest.with_file_name("grid.npz.part");
    std::fs::write(&part, &body()[..40]).unwrap();
    let meta = format!("url: {url}\netag: '\"v1\"'\nlength: 100\n");
    std::fs::write(dest.with_file_name("grid.npz.part.yaml"), meta).unwrap();
}

fn restarts(events: &[Event]) -> Vec<RestartReason> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Restarted { reason, .. } => Some(*reason),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_stopped_download_continues_with_a_range_request_from_the_byte_it_reached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(header("range", "bytes=40-"))
        .and(header("if-range", "\"v1\""))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 40-99/100")
                .insert_header("etag", "\"v1\"")
                .set_body_bytes(&body()[40..]),
        )
        .expect(1)
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    stopped_at_40(&dest, &format!("{}/data/grid.npz", server.uri()));
    let fetched = rig
        .fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "data/grid.npz",
            &dest,
            &Expected::default(),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body());
    assert!(
        rig.events()
            .iter()
            .any(|e| matches!(e, Event::Resumed { from: 40, .. }))
    );
    let Fetched::Arrived(payload) = fetched else {
        panic!("arrived")
    };
    assert_eq!(payload.sha256, hex(&Sha256::digest(body())));
    assert!(
        !dir.path().join("grid.npz.part").exists()
            && !dir.path().join("grid.npz.part.yaml").exists()
    );
}

#[tokio::test]
async fn a_provider_that_ignores_ranges_makes_the_download_start_over_and_says_so() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_bytes(body()),
        )
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    stopped_at_40(&dest, &format!("{}/data/grid.npz", server.uri()));
    rig.fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "data/grid.npz",
            &dest,
            &Expected::default(),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body());
    assert_eq!(restarts(&rig.events()), [RestartReason::RangeIgnored]);
}

#[tokio::test]
async fn a_file_that_changed_since_the_partial_download_starts_over() {
    let changed: Vec<u8> = (100u8..180).collect();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v2\"")
                .set_body_bytes(changed.clone()),
        )
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    stopped_at_40(&dest, &format!("{}/data/grid.npz", server.uri()));
    rig.fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "data/grid.npz",
            &dest,
            &Expected::default(),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), changed);
    assert_eq!(restarts(&rig.events()), [RestartReason::Changed]);
}

#[tokio::test]
async fn a_range_answer_with_another_length_starts_over() {
    let server = MockServer::start().await;
    Mock::given(header("range", "bytes=40-"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 40-119/120")
                .set_body_bytes(vec![0; 80]),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body()))
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    stopped_at_40(&dest, &format!("{}/data/grid.npz", server.uri()));
    rig.fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "data/grid.npz",
            &dest,
            &Expected::default(),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body());
    assert_eq!(restarts(&rig.events()), [RestartReason::Changed]);
}

#[tokio::test]
async fn a_complete_file_is_checked_against_its_published_size_and_digest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    let right = Expected {
        size: Some(100),
        sha256: Some(hex(&Sha256::digest(body())).to_uppercase()),
    };
    rig(false)
        .fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "grid.npz",
            &dest,
            &right,
        )
        .await
        .unwrap();
    std::fs::remove_file(&dest).unwrap();
    let wrong = Expected {
        size: Some(100),
        sha256: Some("0".repeat(64)),
    };
    let error = rig(false)
        .fetcher
        .file(
            &source(),
            "grid",
            &[server.uri()],
            "grid.npz",
            &dest,
            &wrong,
        )
        .await
        .unwrap_err();
    let FetchError::AllMirrorsFailed { errors, .. } = &error else {
        panic!("{error}")
    };
    assert!(
        errors[0].1.contains("does not match the published"),
        "{error}"
    );
    assert!(!dest.exists());
    let short = Expected {
        size: Some(99),
        sha256: None,
    };
    assert!(
        rig(false)
            .fetcher
            .file(
                &source(),
                "grid",
                &[server.uri()],
                "grid.npz",
                &dest,
                &short
            )
            .await
            .is_err()
    );
}

/// A server that sends the first 40 bytes of the file and hangs up, then serves the rest only
/// to a request for range 40 onward.
async fn cut_off_then_resume() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for answer in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let n = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..n]).to_lowercase();
            let reply = if answer == 0 {
                let mut reply =
                    b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\netag: \"v1\"\r\n\r\n".to_vec();
                reply.extend_from_slice(&body()[..40]);
                reply
            } else {
                assert!(request.contains("range: bytes=40-"), "{request}");
                let mut reply = b"HTTP/1.1 206 Partial Content\r\ncontent-length: 60\r\ncontent-range: bytes 40-99/100\r\netag: \"v1\"\r\n\r\n".to_vec();
                reply.extend_from_slice(&body()[40..]);
                reply
            };
            socket.write_all(&reply).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn the_bytes_of_a_cut_off_download_stay_in_its_partial_file_and_the_retry_continues_from_them()
 {
    let mirror = cut_off_then_resume().await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    rig.fetcher
        .file(
            &source(),
            "grid",
            &[mirror],
            "grid.npz",
            &dest,
            &Expected::default(),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body());
    let events = rig.events();
    let err = events
        .iter()
        .position(|e| matches!(e, Event::Err { .. }))
        .unwrap();
    let resumed = events
        .iter()
        .position(|e| matches!(e, Event::Resumed { from: 40, .. }))
        .unwrap();
    assert!(err < resumed);
}
