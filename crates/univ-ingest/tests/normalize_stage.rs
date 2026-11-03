//! The normalize stage reads each source the fetch left on disk, records what became of its
//! rows in its provenance, and reports each source to the run display (F-4, F-13).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

use common::rig;
use univ_ingest::events::{Event, Stage};
use univ_ingest::provenance::Provenance;
use univ_ingest::run::{FetchOptions, RunReport, fetch, normalize_stage};
use univ_ingest::source::{Column, Field, LeftOutReason, SourceDefinition};
use univ_model::units::Unit;
use univ_model::value::Uncertainty;
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Four galaxies, one without a position; the third's zErr of 0 means none was reported.
const ANSWER: &str = "#Table1\nid,ra,dec,z,zErr\nA,10,20,0.01,0.0001\nB,,20,0.02,0.0001\nC,30,-5,0.03,0\nD,40,5,0.04,0.0002\n";

/// A `SkyServer` source of galaxies with a redshift reading and a fix, served by `mirror`.
fn galaxies(dir: &tempfile::TempDir, mirror: &str) -> SourceDefinition {
    let access =
        "  method: skyserver_sql\n  query: \"SELECT id, ra, dec, z, zErr FROM T\"\n  chunks: 1";
    common::definition(dir, access, mirror);
    let path = dir.path().join("test-galaxies.yaml");
    let text = std::fs::read_to_string(&path).unwrap().replace(
        "columns:\n",
        "columns:\n  - { column: id, field: identifier, unit: dimensionless }\n  - { column: z, field: redshift, unit: dimensionless }\n  - { column: zErr, field: redshift_error, unit: dimensionless }\n",
    ) + "fixes:\n  - defect: \"zErr = 0 means no error was measured\"\n    change: \"zErr = 0 is read as none reported\"\n    evidence: \"the test's own convention\"\n    action: { kind: zero_error_is_none_reported, columns: [zErr] }\nnormalization:\n  redshifts:\n    - { column: z, error: zErr, frame: heliocentric, kind: spectroscopic, basis: \"heliocentric by construction\" }\n";
    std::fs::write(&path, text).unwrap();
    SourceDefinition::read(&path).unwrap()
}

async fn serve() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(query_param("format", "csv"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ANSWER))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_provenance_gives_entries_left_out_by_reason_and_each_fix_with_its_count() {
    let server = serve().await;
    let defs = tempfile::tempdir().unwrap();
    let definitions = vec![galaxies(&defs, &server.uri())];
    let cache = tempfile::tempdir().unwrap();
    let options = FetchOptions {
        definitions: defs.path().to_path_buf(),
        repository: defs.path().to_path_buf(),
        cache: cache.path().to_path_buf(),
        sources: vec![],
    };
    let rig = rig(false);
    let report = Mutex::new(RunReport::new(&definitions));
    let acquired = fetch(&rig.fetcher, &definitions, &options, &report).await;
    let normalized = normalize_stage(
        rig.sink.as_ref(),
        &definitions,
        &acquired,
        &options,
        &report,
    )
    .await;

    let entries = &normalized[0].entries;
    assert_eq!(entries.len(), 3);
    let c = entries
        .iter()
        .find(|e| e.designation.identifier == "C")
        .unwrap();
    assert_eq!(
        c.redshifts[0].cmb.heliocentric.uncertainty,
        Uncertainty::NoneReported
    );

    let provenance = Provenance::read(&cache.path().join("test-galaxies")).unwrap();
    let recorded = provenance.normalization.unwrap();
    assert_eq!(recorded.published, 3);
    assert_eq!(
        recorded.left_out,
        BTreeMap::from([(LeftOutReason::MissingPosition, 1)])
    );
    assert_eq!(recorded.fixes.len(), 1);
    assert_eq!(
        (
            recorded.fixes[0].change.as_str(),
            recorded.fixes[0].entries_changed
        ),
        ("zErr = 0 is read as none reported", 1)
    );
    let text = std::fs::read_to_string(cache.path().join("test-galaxies/provenance.yaml")).unwrap();
    assert!(
        text.contains("left_out:\n    missing_position: 1\n"),
        "{text}"
    );

    let events = rig.events();
    let normalize: Vec<&Event> = events
        .iter()
        .skip_while(|e| {
            !matches!(
                e,
                Event::StageStarted {
                    stage: Stage::Normalize,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        normalize,
        [
            &Event::StageStarted {
                stage: Stage::Normalize,
                items: Some(1)
            },
            &Event::Normalized {
                source: common::source(),
                published: 3,
                left_out: 1
            },
            &Event::StageEnded {
                stage: Stage::Normalize,
                counts: vec![]
            },
        ]
    );
    let summary = report.lock().unwrap().sources()[0].clone();
    assert_eq!((summary.published, summary.left_out), (Some(3), Some(1)));
}

#[tokio::test]
async fn an_offline_normalize_prints_each_source_and_the_summary_of_its_entries() {
    let server = serve().await;
    let defs = tempfile::tempdir().unwrap();
    let definition = galaxies(&defs, &server.uri());
    let cache = tempfile::tempdir().unwrap();
    univ_ingest::acquire::acquire(&rig(false).fetcher, &definition, cache.path(), defs.path())
        .await
        .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_univ-ingest"))
        .args(["normalize", "--offline", "--definitions"])
        .arg(defs.path())
        .arg("--cache")
        .arg(cache.path())
        .output()
        .unwrap();
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(run.status.success(), "{log}");
    for line in [
        "Z Stage normalize: 1 items\n",
        "Z Norm test-galaxies: 3 entries, 1 left out\n",
        "Z test-galaxies: 3 entries published, 1 left out\n",
        "Z Stopped at normalize: normalize only; no release was asked for\n",
    ] {
        assert!(log.contains(line), "{line}in\n{log}");
    }
}

#[tokio::test]
async fn a_failed_normalization_leaves_no_earlier_report_in_the_provenance() {
    let server = serve().await;
    let defs = tempfile::tempdir().unwrap();
    let definitions = vec![galaxies(&defs, &server.uri())];
    let cache = tempfile::tempdir().unwrap();
    let options = FetchOptions {
        definitions: defs.path().to_path_buf(),
        repository: defs.path().to_path_buf(),
        cache: cache.path().to_path_buf(),
        sources: vec![],
    };
    let rig = rig(false);
    let report = Mutex::new(RunReport::new(&definitions));
    let acquired = fetch(&rig.fetcher, &definitions, &options, &report).await;
    let sink = rig.sink.as_ref();
    normalize_stage(sink, &definitions, &acquired, &options, &report).await;
    let dir = cache.path().join("test-galaxies");
    assert!(Provenance::read(&dir).unwrap().normalization.is_some());

    // The definition now takes a column the answer lacks, so its normalization fails.
    let mut broken = definitions.clone();
    broken[0].columns.push(Column {
        column: "mag".to_owned(),
        field: Field::Magnitude,
        unit: Unit::Magnitude,
    });
    let normalized = normalize_stage(sink, &broken, &acquired, &options, &report).await;
    assert_eq!(normalized.len(), 0);
    let provenance = Provenance::read(&dir).unwrap();
    assert_eq!(provenance.normalization, None);
    assert_eq!(provenance.payloads.len(), 1, "the payloads stay recorded");
}
