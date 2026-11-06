//! Every payload is recorded with its digest, retrieval time and URL before it is parsed (F-2).

mod common;

use common::{accept, item, keep, rig, source};
use univ_ingest::fetch::{Ask, Fetched};
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
                &item(&format!("chunk {n}"), &dest),
                &[server.uri()],
                &Ask::Get(format!("/q?chunk={n}")),
                &accept,
                &keep,
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

#[test]
fn a_payload_recorded_after_normalization_drops_the_report_it_no_longer_matches() {
    let dir = tempfile::tempdir().unwrap();
    let report = univ_ingest::normalize::NormalizationReport {
        published: 2,
        ..Default::default()
    };
    Provenance::normalized(dir.path(), &source(), &report).unwrap();
    assert_eq!(
        Provenance::read(dir.path()).unwrap().normalization,
        Some(report)
    );
    let payload = univ_ingest::fetch::Payload {
        item: "chunk 1".to_owned(),
        url: "https://example.org/q".to_owned(),
        sha256: "0".repeat(64),
        bytes: 3,
        retrieved: "2026-11-03T00:00:00Z".to_owned(),
        rows: Some(1),
        prepared_by: None,
    };
    Provenance::record(dir.path(), &source(), &payload).unwrap();
    assert_eq!(Provenance::read(dir.path()).unwrap().normalization, None);
}

#[test]
fn payloads_are_kept_in_item_order_whatever_order_they_arrive_in() {
    let dir = tempfile::tempdir().unwrap();
    let payload = |item: &str| univ_ingest::fetch::Payload {
        item: item.to_owned(),
        url: "https://example.org/q".to_owned(),
        sha256: "0".repeat(64),
        bytes: 3,
        retrieved: "2026-11-03T00:00:00Z".to_owned(),
        rows: Some(1),
        prepared_by: None,
    };
    for item in [
        "row count",
        "chunk 10 of 10",
        "chunk 2 of 10",
        "chunk 1 of 10",
    ] {
        Provenance::record(dir.path(), &source(), &payload(item)).unwrap();
    }
    let items: Vec<String> = Provenance::read(dir.path())
        .unwrap()
        .payloads
        .into_iter()
        .map(|p| p.item)
        .collect();
    assert_eq!(
        items,
        [
            "chunk 1 of 10",
            "chunk 2 of 10",
            "chunk 10 of 10",
            "row count"
        ]
    );
}
