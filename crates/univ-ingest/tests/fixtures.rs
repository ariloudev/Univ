//! Each source has a small recorded answer from its real provider, which its access method's
//! reader parses. `record_fixtures` fetches them live and is run by hand:
//! `cargo test -p univ-ingest --test fixtures -- --ignored record_fixtures`.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use univ_ingest::access::{check_answer, chunk_requests};
use univ_ingest::events::RecordingSink;
use univ_ingest::fetch::{Ask, Fetched, Fetcher, Item, RetryPolicy, SystemClock, hex};
use univ_ingest::source::{Access, SourceDefinition};

/// Rows asked of a query provider for a fixture.
const ROWS: u32 = 5;
/// Bytes kept of a file for a fixture: enough for its header.
const BYTES: usize = 4096;

/// What a recording answers, beside it.
#[derive(Debug, Serialize, Deserialize)]
struct Sidecar {
    source: String,
    url: String,
    retrieved: String,
    sha256: String,
    note: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The fixture of a source: `tests/fixtures/<provider>/<id>.<ext>`.
fn fixture(definition: &SourceDefinition) -> Option<PathBuf> {
    let extension = match &definition.access {
        Access::VizierTsv { .. } => "tsv",
        Access::SkyserverSql { .. } | Access::Tap { .. } => "csv",
        Access::Files { .. } => "head",
        Access::Prepared(_) => "csv.gz",
        Access::Repository { .. } => return None,
    };
    let dir = root().join("tests/fixtures").join(&definition.provider);
    Some(dir.join(format!("{}.{extension}", definition.id)))
}

fn sidecar_of(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.yaml",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ))
}

#[test]
fn every_source_has_a_recording_its_reader_parses() {
    for definition in common::read_sources() {
        let Some(path) = fixture(&definition) else {
            continue;
        };
        let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let text = std::fs::read_to_string(sidecar_of(&path))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let sidecar: Sidecar = serde_norway::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            sidecar.sha256,
            hex(&Sha256::digest(&body)),
            "{}",
            path.display()
        );
        assert_eq!(sidecar.source, definition.id.as_str());
        if !matches!(
            definition.access,
            Access::Files { .. } | Access::Prepared(_)
        ) {
            check_answer(&definition, &body).unwrap_or_else(|e| panic!("{}: {e}", definition.id));
        }
    }
}

/// The first chunk's request of a query source, cut to a few rows.
fn small(definition: &SourceDefinition) -> Option<Ask> {
    let cut = |asked: &str| match &definition.access {
        Access::VizierTsv { .. } => {
            asked.replace("-out.max=unlimited", &format!("-out.max={ROWS}"))
        }
        // `SELECT` is the start of the query in the encoded text.
        _ => asked.replacen("SELECT+", &format!("SELECT+TOP+{ROWS}+"), 1),
    };
    Some(match chunk_requests(definition).into_iter().next()?.ask {
        Ask::Get(suffix) => Ask::Get(cut(&suffix)),
        Ask::Job(form) => Ask::Job(cut(&form)),
    })
}

#[tokio::test]
#[ignore = "fetches from the real providers; run by hand to record fixtures"]
async fn record_fixtures() {
    let fetcher = Fetcher::new(
        SystemClock,
        Arc::new(RecordingSink::default()),
        RetryPolicy::default(),
        false,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let client = reqwest::Client::new();
    let mut failures = Vec::new();
    for definition in common::read_sources() {
        let Some(path) = fixture(&definition) else {
            continue;
        };
        if path.exists() {
            continue;
        }
        std::fs::create_dir_all(path.parent().unwrap_or(&path)).unwrap_or_else(|e| panic!("{e}"));
        let (url, body, note) = if let Access::Files { files } = &definition.access {
            let Some(file) = files.first() else { continue };
            let Some(mirror) = definition.mirrors.first() else {
                continue;
            };
            let url = format!(
                "{}/{}",
                mirror.trim_end_matches('/'),
                file.path.trim_start_matches('/')
            );
            let answer = client
                .get(&url)
                .header("range", format!("bytes=0-{}", BYTES - 1))
                .send()
                .await;
            match answer {
                Ok(r) if r.status().is_success() => match r.bytes().await {
                    Ok(b) => (
                        url,
                        b[..b.len().min(BYTES)].to_vec(),
                        format!("the first {BYTES} bytes of the first file"),
                    ),
                    Err(e) => {
                        failures.push(format!("{}: {e}", definition.id));
                        continue;
                    }
                },
                Ok(r) => {
                    failures.push(format!("{}: HTTP {}", definition.id, r.status()));
                    continue;
                }
                Err(e) => {
                    failures.push(format!("{}: {e}", definition.id));
                    continue;
                }
            }
        } else {
            let Some(ask) = small(&definition) else {
                continue;
            };
            let part = path.with_extension("download");
            let check = |b: &[u8]| check_answer(&definition, b);
            let item = Item {
                source: &definition.id,
                name: "fixture",
                dest: &part,
            };
            match fetcher
                .chunk(&item, &definition.mirrors, &ask, &check, &|_| Ok(()))
                .await
            {
                Ok(Fetched::Arrived(payload)) => {
                    let body = std::fs::read(&part).unwrap_or_default();
                    let _ = std::fs::remove_file(&part);
                    (
                        payload.url,
                        body,
                        format!("the first chunk's query cut to {ROWS} rows"),
                    )
                }
                Ok(Fetched::OnDisk) => continue,
                Err(e) => {
                    failures.push(e.to_string());
                    continue;
                }
            }
        };
        let sidecar = Sidecar {
            source: definition.id.to_string(),
            url,
            retrieved: humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string(),
            sha256: hex(&Sha256::digest(&body)),
            note,
        };
        std::fs::write(&path, &body).unwrap_or_else(|e| panic!("{e}"));
        let yaml = serde_norway::to_string(&sidecar).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(sidecar_of(&path), yaml).unwrap_or_else(|e| panic!("{e}"));
        eprintln!("recorded {}", path.display());
    }
    assert!(failures.is_empty(), "not recorded: {failures:#?}");
}
