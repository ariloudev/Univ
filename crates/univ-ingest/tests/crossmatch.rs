//! The cross-match stage of a run (F-13 and the identity stages it joins): SIMULATED entries
//! matched, ranked, given ids against the state directory and placed, with the stage's events
//! and counts; structure members resolved to the matched objects.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod simulated;

use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{TestClock, workspace};
use simulated::{
    designation, entry, matching_rules, offset, ranking_rules, source, with_cz, with_distance,
};
use univ_ingest::crossmatch::{
    Basis, CrossMatched, Inputs, MemberProblem, NameIndex, Resolved, cross_match, resolve_all,
};
use univ_ingest::display::{PlainTarget, RunDisplay, RunSink};
use univ_ingest::events::{Count, Event, EventSink, RecordingSink, Stage};
use univ_ingest::normalize::{NormalizationReport, Normalized};
use univ_ingest::run::{FetchOptions, MatchOptions, RunReport, match_stage};
use univ_ingest::state::{ENTRY_IDS, LEDGER, State, pending};
use univ_ingest::structures::Structure;
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::geometry::{Geometry, Member};
use univ_model::ids::ObjectId;
use univ_model::object::{NotPlacedReason, Object, ObjectEntry, Placement, Resolution};
use univ_model::position::Frame;

const CEPHEIDS: (DistanceKind, Method) = (DistanceKind::PrimaryIndicator, Method::Cepheids);

/// SIMULATED: the LMC in UNGC and `McConnachie` 40" apart (one object, one merge), a 2MRS galaxy
/// between two 6dFGS candidates (two candidate links), and a 6dFGS galaxy without a redshift.
fn entries() -> Vec<CatalogueEntry> {
    let near = offset(80.8938, -69.7561, 40.0, 0.0);
    let (ra, dec) = (200.0, -10.0);
    let (e, w) = (offset(ra, dec, 2.0, 0.0), offset(ra, dec, -2.0, 0.0));
    vec![
        with_distance(
            entry("updated-nearby-galaxies", "LMC", 80.8938, -69.7561),
            0.050,
            Some(0.002),
            CEPHEIDS,
        ),
        with_distance(
            entry("mcconnachie-2012", "LMC", near.0, near.1),
            0.051,
            Some(0.002),
            CEPHEIDS,
        ),
        entry("twomrs", "a", ra, dec),
        with_cz(entry("sixdfgs-dr3", "east", e.0, e.1), 8_040.0, 45.0),
        with_cz(entry("sixdfgs-dr3", "west", w.0, w.1), 7_980.0, 45.0),
        entry("sixdfgs-dr3", "lone", 300.0, -40.0),
    ]
}

fn normalized(entries: Vec<CatalogueEntry>) -> Vec<Normalized> {
    vec![Normalized {
        source: source("sixdfgs-dr3"),
        entries,
        report: NormalizationReport::default(),
    }]
}

fn options(state: &Path) -> (FetchOptions, MatchOptions) {
    let fetch = FetchOptions {
        definitions: workspace().join("sources"),
        repository: workspace(),
        cache: state.join("cache"),
        sources: vec![],
    };
    let state = MatchOptions {
        state: state.to_path_buf(),
        correction: univ_model::redshift::CmbCorrection::new(
            univ_model::redshift::DipoleModel::Planck2018,
        ),
    };
    (fetch, state)
}

async fn run(state: &Path, sink: Arc<dyn EventSink>) -> (Option<CrossMatched>, RunReport) {
    let (fetch, matching) = options(state);
    let report = Mutex::new(RunReport::new(&[]));
    let crossed = match_stage(
        sink,
        &[],
        &[],
        normalized(entries()),
        &fetch,
        &matching,
        &report,
    )
    .await;
    (crossed, report.into_inner().unwrap())
}

fn count(counts: &[Count], what: &str) -> u64 {
    counts.iter().find(|c| c.what == what).unwrap().n
}

#[tokio::test]
async fn the_cross_match_stage_reports_its_steps_and_counts_and_writes_its_ids_as_pending() {
    let dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(RecordingSink::default());
    let (crossed, report) = run(dir.path(), sink.clone()).await;
    let crossed = crossed.unwrap();
    assert!(!report.failed());

    let events = sink.events();
    assert_eq!(
        events[0],
        Event::StageStarted {
            stage: Stage::Match,
            items: Some(5)
        }
    );
    let steps: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Stepped { step, .. } => step.split(':').next(),
            _ => None,
        })
        .collect();
    assert_eq!(
        steps,
        ["matching", "ranking", "ids", "positions", "members"]
    );
    let Some(Event::StageEnded {
        stage: Stage::Match,
        counts,
    }) = events.last()
    else {
        panic!("{events:?}")
    };
    assert_eq!(counts, &crossed.counts());
    for (what, n) in [
        ("entries", 6),
        ("objects", 5),
        ("merges", 1),
        ("candidate links", 2),
        ("new ids", 5),
        ("placed", 3),
        ("not placed (no distance)", 2),
    ] {
        assert_eq!(count(counts, what), n, "{what} in {counts:?}");
    }

    // Each object has its id and, when placed, its position under the release cosmology.
    let lmc = crossed
        .objects
        .iter()
        .find(|o| {
            o.object
                .entry(&designation("mcconnachie-2012", "LMC"))
                .is_some()
        })
        .unwrap();
    assert_eq!(lmc.object.entries.len(), 2);
    let position = lmc.position.as_ref().unwrap();
    assert_eq!(position.cosmology(), Release::planck_2018().cosmology());
    let preferred = lmc.object.preferred_distance.as_ref().unwrap().distance_mpc;
    assert!((position.distance().value - preferred).abs() < 1e-12);
    let lone = crossed
        .objects
        .iter()
        .find(|o| {
            o.object
                .entry(&designation("sixdfgs-dr3", "lone"))
                .is_some()
        })
        .unwrap();
    assert!(lone.position.is_none());
    assert_eq!(
        lone.object.placement,
        Placement::NotPlaced {
            reason: NotPlacedReason::NoDistance
        }
    );

    // The ids go to pending/, for a release to take: the state itself is left as it was.
    assert!(!dir.path().join(LEDGER).exists() && !dir.path().join(ENTRY_IDS).exists());
    let prepared = State::read(&pending(dir.path())).unwrap();
    assert_eq!(prepared.ledger.releases()[0].name, "1");
    assert_eq!(prepared.previous.len(), 6);
    assert_eq!(prepared.previous, crossed.next.previous);

    // The run's summary gives the stage's counts and where it stopped.
    let display = RunSink::new(TestClock::default(), PlainTarget::new(Vec::new()));
    for e in events {
        display.emit(e);
    }
    let (sources, outcome) = report.ended();
    display.finish(sources, outcome);
    let text = String::from_utf8(display.into_inner()).unwrap();
    for line in [
        "Z Step cross-match: matching: 6 entries into 5 objects, 2 candidate links\n",
        "Z cross-match: 6 entries, 5 objects, 1 merges, 2 candidate links, 5 new ids, 0 ids \
         replaced, 3 placed, 2 not placed (no distance), 0 member lists placed, 0 member lists \
         not placed, 0 members resolved by name only\n",
        "Z Stopped at cross-match: cross-match only; no release was asked for\n",
    ] {
        assert!(text.contains(line), "{line} in\n{text}");
    }
}

#[tokio::test]
async fn a_cross_match_again_gives_the_same_ids_until_a_release_takes_them() {
    let dir = tempfile::tempdir().unwrap();
    let first = run(dir.path(), Arc::new(RecordingSink::default())).await;
    let again = run(dir.path(), Arc::new(RecordingSink::default())).await;
    let ids = |c: &Option<CrossMatched>| c.as_ref().unwrap().next.previous.clone();
    assert_eq!(ids(&first.0), ids(&again.0));

    // Once a release has taken the pending state, the next cross-match keeps its ids.
    State::read(&pending(dir.path()))
        .unwrap()
        .write(dir.path())
        .unwrap();
    let next = run(dir.path(), Arc::new(RecordingSink::default())).await;
    let next = next.0.unwrap();
    assert_eq!(next.release, "2");
    assert_eq!(next.next.previous, ids(&first.0));
    assert_eq!(count(&next.counts(), "new ids"), 0);
}

#[tokio::test]
async fn a_state_that_cannot_be_read_fails_the_stage_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(ENTRY_IDS), "version: 1\nentries: []\n").unwrap();
    let sink = Arc::new(RecordingSink::default());
    let (crossed, report) = run(dir.path(), sink.clone()).await;
    assert!(crossed.is_none() && report.failed());
    let (_, outcome) = report.ended();
    let text = format!("{outcome:?}");
    assert!(
        text.contains("ledger.yaml") && text.contains("is missing"),
        "{text}"
    );
    assert!(matches!(
        sink.events().last(),
        Some(Event::StageEnded {
            stage: Stage::Match,
            ..
        })
    ));
}

/// A SIMULATED structure whose member table names `rows`, each `[name, ra, dec]`.
fn structure(rows: &[(&str, f64, f64)]) -> Structure {
    let rows: Vec<String> = rows
        .iter()
        .map(|(name, ra, dec)| format!("      - [\"{name}\", {ra}, {dec}]\n"))
        .collect();
    let rows = rows.concat();
    let yaml = format!(
        "name: Test Pair\nkind: wall\nstatus: observed\nscale_domain: cosmic_web\npapers:\n  - id: test-2026\n    reference: \"Test 2026, ApJ 1, 1\"\n    title: \"Simulated\"\n    doi: 10.0000/test\n    read: \"simulated for this test\"\ngeometry:\n  kind: members\n  members:\n    frame: icrs\n    tracer: galaxy\n    source: {{ paper: test-2026, table: \"1\" }}\n    columns:\n      - {{ name: lon, header: \"RA\", unit: degree, kind: measured, uncertainty: {{ form: none_reported }} }}\n      - {{ name: lat, header: \"Dec\", unit: degree, kind: measured, uncertainty: {{ form: none_reported }} }}\n    rows:\n{rows}notes: \"Simulated.\"\n"
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test-pair.yaml");
    std::fs::write(&file, yaml).unwrap();
    Structure::read(&file).unwrap()
}

fn cross(structures: &[Structure]) -> CrossMatched {
    let mut entries = entries();
    entries.push(with_distance(
        entry("updated-nearby-galaxies", "SMC", 13.1867, -72.8286),
        0.062,
        Some(0.003),
        CEPHEIDS,
    ));
    let basis = Basis {
        matching: matching_rules(),
        ranking: ranking_rules(),
        structures: structures
            .iter()
            .map(|s| ("test-pair".to_owned(), s.clone()))
            .collect(),
        ..Basis::read(&workspace().join("rules"), &[]).unwrap()
    };
    let (release, state) = (Release::planck_2018(), State::default());
    let inputs = Inputs {
        basis: &basis,
        release: &release,
        correction: &univ_model::redshift::CmbCorrection::new(
            univ_model::redshift::DipoleModel::Planck2018,
        ),
        state: &state,
    };
    cross_match(entries, &inputs, &RecordingSink::default()).unwrap()
}

#[test]
fn members_that_all_resolve_to_placed_objects_are_cached_with_their_bounding_box() {
    let pair = structure(&[("LMC", 80.8938, -69.7561), ("  smc ", 13.1867, -72.8286)]);
    let crossed = cross(&[pair]);
    let placed = &crossed.structures[0];
    assert!(placed.unplaced.is_empty() && placed.point.is_none());
    let shape = placed.shape.as_ref().unwrap();
    let Geometry::Members { members } = &shape.geometry else {
        panic!("{shape:?}")
    };
    let ids: Vec<_> = members
        .iter()
        .map(|m| match m {
            Member::Object { id, .. } => *id,
            Member::Tracer { .. } => panic!("a tracer"),
        })
        .collect();
    let id_of = |d| crossed.next.previous[&designation("updated-nearby-galaxies", d)];
    assert_eq!(ids, [id_of("LMC"), id_of("SMC")]);
    let bounds = placed.bounds.unwrap();
    assert_eq!(
        shape.bounds_in(Frame::Icrs),
        Some(bounds),
        "no lookup needed"
    );
    for m in members {
        let Member::Object { at, .. } = m else {
            unreachable!()
        };
        assert!((0..3).all(|k| bounds.min[k] <= at[k] && at[k] <= bounds.max[k]));
    }
}

#[test]
fn a_member_that_resolves_to_no_object_or_an_unplaced_one_keeps_the_structure_a_point() {
    let lone = (300.0, -40.0);
    let rows = [
        ("LMC", 80.8938, -69.7561),
        ("Ghost", 10.0, 10.0),
        ("2", 13.1867, -72.8286),
        ("lone", lone.0, lone.1),
        ("SMC", 100.0, 0.0),
    ];
    let crossed = cross(&[structure(&rows)]);
    let placed = &crossed.structures[0];
    assert!(placed.shape.is_none() && placed.bounds.is_none());
    assert_eq!(
        placed.point, None,
        "the test structure publishes no position"
    );
    let why: Vec<(&str, &MemberProblem)> = placed
        .unplaced
        .iter()
        .map(|m| (m.member.as_str(), &m.why))
        .collect();
    assert_eq!(why.len(), 4, "{why:?}");
    assert_eq!(why[0], ("Ghost", &MemberProblem::NoObject));
    assert_eq!(
        why[1],
        ("2", &MemberProblem::NoObject),
        "a bare number names nothing"
    );
    assert!(matches!(
        why[2],
        (
            "lone",
            MemberProblem::Unplaced {
                reason: NotPlacedReason::NoDistance,
                ..
            }
        )
    ));
    assert!(matches!(why[3], ("SMC", MemberProblem::TooFar { arcsec, .. }) if *arcsec > 60.0));
    assert_eq!(placed.unresolved_and_unplaced(), (3, 1));
    assert_eq!(count(&crossed.counts(), "member lists not placed"), 1);
}

#[test]
fn a_member_whose_position_cannot_be_checked_is_resolved_by_name_alone_and_counted() {
    // A placed object without a preferred sky position to check the member's against: the
    // member is resolved by its name alone, recorded as unchecked, and the stage counts it.
    let galaxy = with_distance(
        entry("updated-nearby-galaxies", "Lonely", 10.0, 10.0),
        5.0,
        Some(0.2),
        CEPHEIDS,
    );
    let release = Release::planck_2018();
    let at = release.place(galaxy.position.as_ref(), galaxy.distances.first(), None);
    let at = at.unwrap().unwrap();
    let mut object = Object::new(vec![ObjectEntry {
        entry: galaxy,
        resolved_by: Resolution::Anchor.into(),
    }]);
    object.placement = Placement::Placed;
    let resolved = [Resolved {
        id: ObjectId::new(1).unwrap(),
        object: &object,
        position: Some(&at),
    }];
    let tolerance = matching_rules().structure_members.angular_tolerance_arcsec;
    assert_eq!(tolerance, 60.0);
    let wall = structure(&[("Lonely", 50.0, -30.0)]);
    let lists = resolve_all(
        std::slice::from_ref(&wall),
        &NameIndex::new(&resolved),
        tolerance,
        None,
    );
    assert_eq!(lists[0].by_name_only, ["Lonely"]);
    assert!(lists[0].shape.is_some() && lists[0].unplaced.is_empty());
    let mut crossed = cross(&[wall]);
    crossed.structures = lists;
    assert_eq!(count(&crossed.counts(), "members resolved by name only"), 1);
}
