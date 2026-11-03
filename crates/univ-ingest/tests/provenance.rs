//! Every payload is recorded with its digest, retrieval time and URL before it is parsed (F-2).

mod common;

use common::{accept, rig, source};
use univ_ingest::fetch::Fetched;
use univ_ingest::provenance::{FILE, Provenance};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn the_provenance_lists_each_payload_with_its_digest_retrieval_time_and_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("abc"))
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    for n in 0..2 {
        let dest = dir.path().join(format!("chunk-{n}.tsv"));
        let fetched = rig
            .fetcher
            .chunk(
                &source(),
                &format!("chunk {n}"),
                &[server.uri()],
                &format!("/q?chunk={n}"),
                &dest,
                &accept,
            )
            .await
            .unwrap();
        let Fetched::Arrived(payload) = fetched else {
            panic!("arrived")
        };
        Provenance::record(dir.path(), &source(), &payload).unwrap();
    }
    let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    let provenance = Provenance::read(dir.path()).unwrap();
    assert_eq!(provenance.source, Some(source()));
    assert_eq!(provenance.payloads.len(), 2, "{text}");
    let first = &provenance.payloads[0];
    assert_eq!(first.item, "chunk 0");
    assert_eq!(first.url, format!("{}/q?chunk=0", server.uri()));
    // SHA-256 of "abc".
    assert_eq!(
        first.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        (first.bytes, first.retrieved.as_str()),
        (3, "2026-11-03T00:00:00Z")
    );
}
