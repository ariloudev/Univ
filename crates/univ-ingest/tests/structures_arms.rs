//! The arms the spiral-arm fits are stretches of: every paper's fit of one arm names that arm by
//! one name, so that the fits of Reid et al. 2019 and of the papers that reach beyond it are
//! known as one arm (record-a-researched-structure).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use common::workspace;
use univ_ingest::structures::{Arm, LogSpiral, Structure, StructureGeometry, read_all_named};

/// Every log-spiral structure of the repository, read once, by id.
static SPIRALS: LazyLock<Vec<(String, LogSpiral)>> = LazyLock::new(|| {
    read_all_named(&workspace().join("structures"))
        .unwrap()
        .into_iter()
        .filter_map(|(id, s)| match s.geometry {
            Some(StructureGeometry::LogSpiral(l)) => Some((id, *l)),
            _ => None,
        })
        .collect()
});

/// The arm a structure's id names, as `<paper>-<arm>-arm` or `<paper>-<arm>-arm-extrapolated`.
fn arm_of_id(id: &str) -> &str {
    let stem = id.strip_suffix("-extrapolated").unwrap_or(id);
    let stem = stem.strip_suffix("-arm").unwrap();
    Arm::ALL
        .iter()
        .map(|a| a.slug())
        .filter(|slug| stem.ends_with(&format!("-{slug}")))
        .max_by_key(|slug| slug.len())
        .unwrap()
}

#[test]
fn each_spiral_arm_file_names_the_arm_its_id_names() {
    assert!(SPIRALS.len() >= 14);
    for (id, l) in SPIRALS.iter() {
        assert_eq!(l.arm.slug(), arm_of_id(id), "{id}");
    }
}

#[test]
fn a_fit_and_its_extrapolation_name_one_arm() {
    let by_id: BTreeMap<&str, Arm> = SPIRALS.iter().map(|(i, l)| (i.as_str(), l.arm)).collect();
    for (id, arm) in &by_id {
        if let Some(fitted) = id.strip_suffix("-extrapolated") {
            assert_eq!(by_id.get(fitted), Some(arm), "{id}");
        }
    }
}

#[test]
fn every_arm_of_reid_2019s_table_2_is_named_by_its_fit() {
    let named: Vec<Arm> = SPIRALS
        .iter()
        .filter(|(id, _)| id.starts_with("reid-2019-"))
        .map(|(_, l)| l.arm)
        .collect();
    for arm in [
        Arm::ThreeKpcNear,
        Arm::Norma,
        Arm::ScutumCentaurus,
        Arm::SagittariusCarina,
        Arm::Local,
        Arm::Perseus,
        Arm::Outer,
    ] {
        assert_eq!(named.iter().filter(|a| **a == arm).count(), 2, "{arm:?}");
    }
}

/// The Norma arm's file as written, `edit` applied to it.
fn edited(edit: impl Fn(String) -> String) -> Result<Structure, String> {
    let text =
        std::fs::read_to_string(workspace().join("structures/reid-2019-norma-arm.yaml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test-arm.yaml");
    std::fs::write(&file, edit(text)).unwrap();
    Structure::read(&file).map_err(|e| e.to_string())
}

#[test]
fn a_spiral_arm_without_an_arm_or_naming_an_unknown_arm_is_refused() {
    assert!(edited(|t| t).is_ok());
    let missing = edited(|t| t.replace("    arm: norma\n", "")).unwrap_err();
    assert!(missing.contains("arm"), "{missing}");
    let unknown = edited(|t| t.replace("arm: norma", "arm: cygnus")).unwrap_err();
    assert!(unknown.contains("cygnus"), "{unknown}");
}
