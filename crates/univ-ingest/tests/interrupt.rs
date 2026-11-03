//! An interrupted run keeps what arrived, and the next run continues from it (F-13).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers unwrap or panic: a failure there is the test failing"
)]

mod common;

use std::time::Duration;

use common::{item, keep, rig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use univ_ingest::events::Event;
use univ_ingest::fetch::Expected;
use wiremock::matchers::header;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn body() -> Vec<u8> {
    (0u8..100).collect()
}

/// A server that sends the first 40 bytes of the file and then stalls, as a slow provider
/// does when the owner gives up and presses Ctrl-C.
async fn stalling() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let mut reply = b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\netag: \"v1\"\r\n\r\n".to_vec();
        reply.extend_from_slice(&body()[..40]);
        socket.write_all(&reply).await.unwrap();
        tokio::time::sleep(Duration::from_secs(3_600)).await;
    });
    format!("http://{address}")
}

async fn until_part_holds(path: &std::path::Path, bytes: u64) {
    for _ in 0..500 {
        if std::fs::metadata(path).is_ok_and(|m| m.len() >= bytes) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{} never held {bytes} bytes", path.display());
}

#[tokio::test]
async fn an_interrupted_download_keeps_its_bytes_and_the_next_run_continues_from_them() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("grid.npz");
    let part = dir.path().join("grid.npz.part");
    let mirror = stalling().await;
    let first = rig(false);
    // The owner interrupts the run once 40 bytes have arrived: the fetch is dropped mid-body.
    let grid = item("grid", &dest);
    let mirrors = [mirror.clone()];
    let whole = Expected::default();
    tokio::select! {
        result = first.fetcher.file(&grid, &mirrors, "grid.npz", &whole, &keep) => {
            panic!("the stalled download finished: {result:?}");
        }
        () = until_part_holds(&part, 40) => {}
    }
    assert!(!dest.exists(), "nothing half fetched is taken for whole");
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 40);

    let server = MockServer::start().await;
    Mock::given(header("range", "bytes=40-"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 40-99/100")
                .insert_header("etag", "\"v1\"")
                .set_body_bytes(&body()[40..]),
        )
        .expect(1)
        .mount(&server)
        .await;
    // The record of the partial download names the stalled mirror; the next run reaches the
    // file through the same path, so point its record at the new server as a mirror move would.
    let meta = dir.path().join("grid.npz.part.yaml");
    let record = std::fs::read_to_string(&meta)
        .unwrap()
        .replace(&mirror, &server.uri());
    std::fs::write(&meta, record).unwrap();
    let next = rig(false);
    next.fetcher
        .file(
            &item("grid", &dest),
            &[server.uri()],
            "grid.npz",
            &Expected::default(),
            &keep,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body());
    assert!(
        next.events()
            .iter()
            .any(|e| matches!(e, Event::Resumed { from: 40, .. }))
    );
}
