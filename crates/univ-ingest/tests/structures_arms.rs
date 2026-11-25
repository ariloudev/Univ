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

use common::{edited_norma_arm, structures_dir};
use univ_ingest::structures::{Arm, LogSpiral, StructureGeometry, read_all_named};

/// Every log-spiral structure of the repository, read once, by id.
static SPIRALS: LazyLock<Vec<(String, LogSpiral)>> = LazyLock::new(|| {
    read_all_named(&structures_dir())
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

#[test]
fn a_spiral_arm_without_an_arm_or_naming_an_unknown_arm_is_refused() {
    assert!(edited_norma_arm(|t| t).is_ok());
    let missing = edited_norma_arm(|t| t.replace("    arm: norma\n", "")).unwrap_err();
    assert!(missing.contains("arm"), "{missing}");
    let unknown = edited_norma_arm(|t| t.replace("arm: norma", "arm: cygnus")).unwrap_err();
    assert!(unknown.contains("cygnus"), "{unknown}");
}

/// The paper a spiral's id names first, up to its year, such as `sun-2024` for
/// `sun-2024-outer-arm` or `mcclure-griffiths-2004` for `mcclure-griffiths-2004-outer-arm`: the
/// paper whose fit it is.
fn paper_of_id(id: &str) -> &str {
    let mut end = 0;
    for part in id.split('-') {
        end += part.len();
        if part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()) {
            return &id[..end];
        }
        end += 1;
    }
    id
}

#[test]
fn each_fits_parameters_cite_where_the_paper_whose_fit_it_is_gives_them() {
    for (id, l) in SPIRALS.iter() {
        let paper = paper_of_id(id);
        let own: Vec<(&str, &univ_ingest::structures::Cited)> = l
            .scalars()
            .filter(|(name, _)| !matches!(*name, "r0" | "sun_height"))
            .chain(l.fitted.iter().map(|c| ("fitted", c)))
            .collect();
        for (name, c) in own {
            assert_eq!(c.cite.paper, paper, "{id}: {name}");
        }
    }
}

#[test]
fn each_fit_gives_the_galactic_centres_distance_it_assumes_and_the_suns_height_cited() {
    for (id, l) in SPIRALS.iter() {
        let r0 =
            l.r0.quantity
                .to(univ_model::units::Unit::Kiloparsec)
                .unwrap()
                .value;
        let expected = if id.starts_with("mcclure-griffiths-2004-") {
            // McClure-Griffiths et al. 2004 adopt the IAU's R0.
            8.5
        } else {
            // Reid et al. 2019's fit A5, which Sun et al. 2024 and Hyland et al. 2026 adopt.
            8.15
        };
        assert!((r0 - expected).abs() < 1e-9, "{id}: {r0}");
        assert!(!l.r0.cite.paper.is_empty() && !l.sun_height.cite.paper.is_empty());
    }
}

#[test]
fn each_path_is_computed_with_the_galactic_centres_distance_its_fit_assumes() {
    for (id, l) in SPIRALS.iter() {
        let r0 =
            l.r0.quantity
                .to(univ_model::units::Unit::Kiloparsec)
                .unwrap()
                .value;
        for (beta, p) in l.azimuths().into_iter().zip(l.path_galactic_kpc()) {
            let r = l.radius_kpc(beta);
            let b = beta.to_radians();
            assert!((p[0] - (r0 - r * b.cos())).abs() < 1e-9, "{id} at {beta}");
            assert!((p[1] - r * b.sin()).abs() < 1e-9, "{id} at {beta}");
        }
    }
}

#[test]
fn a_fit_of_another_paper_than_reid_2019_says_how_its_azimuth_is_read_in_reids_convention() {
    for (id, l) in SPIRALS.iter() {
        if !id.starts_with("reid-2019-") {
            let convention = l.convention.as_deref().unwrap_or_default();
            assert!(!convention.trim().is_empty(), "{id}");
        }
    }
}

/// The heliocentric galactic longitude, degrees, and distance, kpc, of `l`'s path at `beta`.
fn sight(spiral: &LogSpiral, beta: f64) -> (f64, f64) {
    let r0 = spiral
        .r0
        .quantity
        .to(univ_model::units::Unit::Kiloparsec)
        .unwrap()
        .value;
    let (radius, azimuth) = (spiral.radius_kpc(beta), beta.to_radians());
    let (x, y) = (r0 - radius * azimuth.cos(), radius * azimuth.sin());
    (y.atan2(x).to_degrees().rem_euclid(360.0), x.hypot(y))
}

#[test]
fn the_far_side_fits_lie_where_their_papers_place_their_arms() {
    let get = |id: &str| &SPIRALS.iter().find(|(i, _)| i == id).unwrap().1;
    // Sun et al. 2024: the outer Scutum-Centaurus arm's clouds run from the third quadrant, at
    // l ≈ 220°, to the first, from l ≈ 15°.
    let osc = get("sun-2024-scutum-centaurus-arm");
    let (l_low, _) = sight(osc, -27.3);
    let (l_high, d_high) = sight(osc, 155.8);
    assert!((200.0..235.0).contains(&l_low), "{l_low}");
    assert!(
        (10.0..20.0).contains(&l_high) && d_high > 15.0,
        "{l_high} {d_high}"
    );
    // Hyland et al. 2026: Perseus reaches β ≈ 180°, beyond the Galactic Centre toward l ≈ 0°.
    let perseus = get("hyland-2026-perseus-arm");
    let (l_far, d_far) = sight(perseus, 180.0);
    assert!(l_far < 5.0 || l_far > 355.0, "{l_far}");
    assert!((12.0..16.0).contains(&d_far), "{d_far}");
    // McClure-Griffiths et al. 2004: the arm lies in the fourth quadrant, l ≈ 253° to 321°.
    let distant = get("mcclure-griffiths-2004-outer-arm");
    let [low, high] = distant.fitted.clone().map(|c| c.value());
    for beta in [low, high] {
        let (l, _) = sight(distant, beta);
        assert!((240.0..330.0).contains(&l), "{beta}: {l}");
    }
}
