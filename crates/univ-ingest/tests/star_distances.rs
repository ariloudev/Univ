//! Researched star distances (record-a-researched-star-distance): a star file is read with
//! every number cited, joins its Hipparcos star by HIP number and places it; an uncited number
//! is refused; a star whose papers publish no distance keeps its file and adds nothing.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod simulated;

use std::path::Path;

use common::{definition_of, workspace};
use simulated::{entry, matching_rules, ranking_rules};
use univ_ingest::crossmatch::{Basis, Inputs, cross_match};
use univ_ingest::events::RecordingSink;
use univ_ingest::source::LeftOutReason;
use univ_ingest::stars::{normalize, problems, read_all};
use univ_ingest::state::State;
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::object::{Decided, Placement};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

/// A star file of HIP `hip` whose distance is `distance`, as written in `body`.
fn star(hip: u32, distance: &str) -> String {
    format!(
        "hip: {hip}\nname: \"Test star\"\npapers:\n  - id: test-2020\n    reference: \"Test 2020, SIMULATED 1, 1\"\n    title: \"t\"\n    bibcode: 2020SIM.....1....1T\n    read: \"SIMULATED\"\n{distance}notes: \"SIMULATED.\"\n"
    )
}

const MODEL: &str = "distance:\n  value: 168\n  unit: parsec\n  kind: estimated\n  uncertainty: { form: asymmetric, minus: 15, plus: 27 }\n  cite:\n    paper: test-2020\n    sentence: \"SIMULATED: a distance of 168^{+27}_{-15} pc.\"\n  distance_kind: primary_indicator\n  method: \"SIMULATED model\"\n";

fn dir_of(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in files {
        std::fs::write(dir.path().join(format!("{name}.yaml")), body).unwrap();
    }
    dir
}

fn refusal(body: &str) -> String {
    let dir = dir_of(&[("test-star", body.to_owned())]);
    read_all(dir.path()).unwrap_err().to_string()
}

#[test]
fn a_star_file_joins_its_hipparcos_star_and_places_it_by_its_distance() {
    let definition = definition_of("researched-star-distances");
    let dir = dir_of(&[("betelgeuse", star(27_989, MODEL))]);
    let researched = normalize(&definition, dir.path()).unwrap();
    // Betelgeuse in Hipparcos-2: a parallax of 6.55 ± 0.83 mas, too uncertain to invert.
    let mut hipparcos = entry("hipparcos-2007", "27989", 88.7929, 7.4070);
    let parallax = Quantity::new(
        6.55,
        Unit::Milliarcsecond,
        Uncertainty::Symmetric { sigma: 0.83 },
    )
    .unwrap();
    hipparcos.parallax =
        Some(Value::normalize(parallax, Unit::Degree, KindOfValue::Measured).unwrap());
    let mut entries = vec![hipparcos];
    entries.extend(researched.entries);
    let basis = Basis {
        matching: matching_rules(),
        ranking: ranking_rules(),
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
    let crossed = cross_match(entries, &inputs, &RecordingSink::default()).unwrap();
    assert_eq!(crossed.objects.len(), 1, "one object of both entries");
    let object = &crossed.objects[0].object;
    assert_eq!(object.placement, Placement::Placed);
    let preferred = object.preferred_distance.as_ref().unwrap();
    let by = preferred.measurement.entry();
    let chosen = &object
        .entries
        .iter()
        .find(|e| &e.entry.designation == by)
        .unwrap()
        .entry;
    assert_eq!(
        chosen.designation.source.as_str(),
        "researched-star-distances"
    );
    assert!((preferred.distance_mpc * 1e6 - 168.0).abs() < 1e-6);
    assert_eq!(preferred.decided, Decided::OnlyCandidate);
    let papers = &*chosen.references;
    assert_eq!(papers[0].citation, "Test 2020, SIMULATED 1, 1");
}

#[test]
fn a_star_files_value_keeps_its_kind_uncertainty_and_method() {
    let definition = definition_of("researched-star-distances");
    let parallax = "parallax:\n  value: 2.5\n  unit: milliarcsecond\n  kind: measured\n  uncertainty: { form: symmetric, sigma: 0.1 }\n  cite:\n    paper: test-2020\n    sentence: \"SIMULATED: a parallax of 2.5 ± 0.1 mas.\"\n  distance_kind: parallax\n  method: \"SIMULATED orbit\"\n";
    let dir = dir_of(&[("one", star(1, MODEL)), ("two", star(2, parallax))]);
    let entries = normalize(&definition, dir.path()).unwrap().entries;
    let d = &entries[0].distances[0];
    assert_eq!(d.kind_of_value, KindOfValue::Estimated);
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(d.method, Method::Other("SIMULATED model".to_owned()));
    assert_eq!(
        d.native.uncertainty,
        Uncertainty::Asymmetric {
            minus: 15.0,
            plus: 27.0
        }
    );
    let p = entries[1].parallax.as_ref().unwrap();
    assert_eq!(p.kind, KindOfValue::Measured);
    assert!(matches!(
        p.native.uncertainty,
        Uncertainty::Symmetric { .. }
    ));
    assert_eq!(entries[1].flags["method"], "SIMULATED orbit");
}

#[test]
fn a_number_its_quoted_sentence_does_not_hold_is_refused_naming_the_field() {
    let refused = refusal(&star(1, &MODEL.replace("value: 168", "value: 170")));
    assert!(
        refused.contains("distance") && refused.contains("170"),
        "{refused}"
    );
}

#[test]
fn a_number_of_the_notes_that_no_citation_holds_is_refused() {
    let refused = refusal(&star(1, MODEL).replace("SIMULATED.", "SIMULATED, at 190 pc."));
    assert!(
        refused.contains("notes") && refused.contains("190"),
        "{refused}"
    );
}

#[test]
fn a_groups_distance_is_inferred_and_cites_the_membership() {
    let inferred = MODEL.replace("kind: estimated", "kind: inferred");
    let refused = refusal(&star(1, &inferred));
    assert!(refused.contains("membership"), "{refused}");
    let member = format!(
        "{inferred}membership:\n  paper: test-2020\n  sentence: \"SIMULATED: the star is a member.\"\n"
    );
    let dir = dir_of(&[("test-star", star(1, &member))]);
    assert_eq!(read_all(dir.path()).unwrap().len(), 1);
}

#[test]
fn a_membership_beside_a_value_that_is_no_groups_distance_is_refused() {
    let member = format!(
        "{MODEL}membership:\n  paper: test-2020\n  sentence: \"SIMULATED: the star is a member.\"\n"
    );
    let refused = refusal(&star(1, &member));
    assert!(refused.contains("membership"), "{refused}");
}

#[test]
fn a_parallax_the_ranking_could_not_invert_is_refused() {
    let parallax = "parallax:\n  value: 5.95\n  unit: milliarcsecond\n  kind: measured\n  uncertainty: { form: asymmetric, minus: 0.85, plus: 0.58 }\n  cite:\n    paper: test-2020\n    sentence: \"SIMULATED: a parallax of 5.95^{+0.58}_{-0.85} mas.\"\n  distance_kind: parallax\n  method: \"SIMULATED orbit\"\n";
    let refused = refusal(&star(1, parallax));
    assert!(refused.contains("parallax.uncertainty"), "{refused}");
    let zero = MODEL.replace("value: 168", "value: 0");
    let refused = refusal(&star(1, &zero));
    assert!(refused.contains("is not positive"), "{refused}");
}

#[test]
fn a_star_without_a_published_distance_keeps_its_file_and_adds_no_entry() {
    let unplaced =
        "unplaced: \"No paper publishes a distance other than the Hipparcos parallax.\"\n";
    let dir = dir_of(&[("test-star", star(1, unplaced))]);
    let n = normalize(&definition_of("researched-star-distances"), dir.path()).unwrap();
    assert_eq!(n.entries, []);
    assert_eq!(
        n.report.left_out.get(&LeftOutReason::NoPublishedDistance),
        Some(&1)
    );
    let refused = refusal(&star(1, ""));
    assert!(refused.contains("unplaced"), "{refused}");
}

#[test]
fn every_star_file_of_the_repository_and_of_the_tests_can_be_used() {
    for dir in [
        workspace().join("stars"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stars"),
    ] {
        let problems = problems(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(problems, [] as [String; 0], "{}", problems.join("\n"));
    }
}

/// Every problem of the star files of the directory `UNIV_STARS` names: what the researchers
/// of star files run on their drafts before the files join `stars/`.
#[test]
#[ignore = "checks the star files of the directory UNIV_STARS names; run by hand"]
fn every_star_file_of_a_directory_can_be_used() {
    let dir = std::path::PathBuf::from(std::env::var("UNIV_STARS").unwrap());
    let problems = problems(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        problems,
        [] as [String; 0],
        "\nPROBLEM {}",
        problems.join("\nPROBLEM ")
    );
}
