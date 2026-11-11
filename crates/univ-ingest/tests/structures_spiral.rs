//! The Galaxy's model spiral arms: each arm of Reid et al. 2019's Table 2 as a spiral arm over its
//! fitted azimuths and as an extrapolation beyond them, computed from its cited log-spiral
//! parameters (record-a-researched-structure; read-a-structure).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::sync::LazyLock;

use common::workspace;
use univ_ingest::release::{Placing, placed_shape};
use univ_ingest::source::ScaleDomain;
use univ_ingest::structures::{
    EXTRAPOLATION_DEGREES, Extent, LogSpiral, RADIUS_BAND_KPC, Status, Structure,
    StructureGeometry, StructureKind, read_all_named,
};
use univ_model::derive::{Release, Transformation};
use univ_model::geometry::Geometry;
use univ_model::position::{Frame, Vec3, between};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

/// The repository's model arms, read once, by id.
static ARMS: LazyLock<Vec<(String, Structure)>> = LazyLock::new(|| {
    read_all_named(&workspace().join("structures"))
        .unwrap()
        .into_iter()
        .filter(|(id, _)| id.starts_with("reid-2019-"))
        .collect()
});

fn spiral(id: &str) -> LogSpiral {
    let s = &ARMS.iter().find(|(i, _)| i == id).unwrap().1;
    // A log spiral, or the test fails here.
    match &s.geometry {
        Some(StructureGeometry::LogSpiral(l)) => Some((**l).clone()),
        _ => None,
    }
    .unwrap()
}

/// The in-plane distance, kpc, from `at` to the nearest point of `path`.
fn nearest(path: &[Vec3], at: [f64; 2]) -> f64 {
    path.iter()
        .map(|p| (p[0] - at[0]).hypot(p[1] - at[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Heliocentric galactic x, y in kpc of a source at `l`, `b` degrees and parallax `mas`.
fn galactic(l: f64, b: f64, mas: f64) -> [f64; 2] {
    let d = 1.0 / mas;
    let (l, b) = (l.to_radians(), b.to_radians());
    [d * b.cos() * l.cos(), d * b.cos() * l.sin()]
}

#[test]
fn each_arm_of_table_2_is_a_model_spiral_arm_and_an_extrapolation_among_the_galaxy_tracers() {
    assert_eq!(
        ARMS.len(),
        14,
        "{:?}",
        ARMS.iter().map(|(i, _)| i).collect::<Vec<_>>()
    );
    for (id, s) in ARMS.iter() {
        let Some(StructureGeometry::LogSpiral(l)) = &s.geometry else {
            panic!("{id}")
        };
        let extrapolated = id.ends_with("-extrapolated");
        assert_eq!(l.extent == Extent::Extrapolated, extrapolated, "{id}");
        let kind = if extrapolated {
            StructureKind::SpiralArmExtrapolation
        } else {
            StructureKind::SpiralArm
        };
        assert_eq!(s.kind, kind, "{id}");
        assert_eq!(s.status, Status::Model, "{id}");
        assert_eq!(s.scale_domain, ScaleDomain::GalaxyTracers, "{id}");
        assert!(s.name.contains("(Reid 2019 model)"), "{id}: {}", s.name);
        assert_eq!(s.geometry_kind(), Some("path"), "{id}");
        // Every arm assumes the Galactic Centre's distance of Table 2.
        assert_eq!(
            (l.r0.value(), l.r0.unit()),
            (8.15, Unit::Kiloparsec),
            "{id}"
        );
    }
}

#[test]
fn the_formula_gives_the_radii_of_the_arms_toward_the_sun() {
    // ln(R / R_kink) = -(beta - beta_kink) tan(psi) at beta = 0: the Local arm just outside the
    // Sun's circle, Perseus beyond it and Sagittarius-Carina inside it, with R0 = 8.15 kpc.
    for (id, r) in [
        ("reid-2019-local-arm", 8.526),
        ("reid-2019-perseus-arm", 10.070),
        ("reid-2019-sagittarius-carina-arm", 6.871),
    ] {
        let at = spiral(id).radius_kpc(0.0);
        assert!((at - r).abs() < 0.001, "{id}: {at}");
    }
    // The pitch angle is the one on the azimuth's side of the kink: Sagittarius-Carina winds
    // at 17.1 degrees inside its kink at 24 and at 1.0 degree outside it.
    let sgr = spiral("reid-2019-sagittarius-carina-arm");
    let (at_kink, beyond) = (sgr.radius_kpc(24.0), sgr.radius_kpc(84.0));
    assert!((at_kink - 6.04).abs() < 1e-9);
    let expected = 6.04 * (-(60f64.to_radians()) * 1f64.to_radians().tan()).exp();
    assert!((beyond - expected).abs() < 1e-9, "{beyond}");
}

#[test]
fn a_spiral_arm_spans_its_fitted_azimuths_and_no_more() {
    let perseus = spiral("reid-2019-perseus-arm");
    let betas = perseus.azimuths();
    assert_eq!(betas.first(), Some(&-23.0));
    assert_eq!(betas.last(), Some(&115.0));
    assert_eq!(betas.len(), 139, "each whole degree from -23 to 115");
    assert!(betas.windows(2).all(|w| (w[1] - w[0] - 1.0).abs() < 1e-9));
    assert_eq!(perseus.path_galactic_kpc().len(), betas.len());
}

#[test]
fn an_extrapolation_runs_on_up_to_90_degrees_while_its_radius_lies_within_3_to_16_kpc() {
    for (id, s) in ARMS.iter().filter(|(id, _)| id.ends_with("-extrapolated")) {
        let Some(StructureGeometry::LogSpiral(l)) = &s.geometry else {
            panic!("{id}")
        };
        let [low, high] = [l.fitted[0].value(), l.fitted[1].value()];
        let betas = l.azimuths();
        let (first, last) = (betas[0], *betas.last().unwrap());
        assert!(
            first <= low && low - first <= EXTRAPOLATION_DEGREES,
            "{id}: {first}"
        );
        assert!(
            last >= high && last - high <= EXTRAPOLATION_DEGREES,
            "{id}: {last}"
        );
        // It passes through its fitted range, both ends included.
        assert!(betas.contains(&low) && betas.contains(&high), "{id}");
        for beta in betas.iter().filter(|b| **b < low || **b > high) {
            let r = l.radius_kpc(*beta);
            assert!(
                (RADIUS_BAND_KPC.0..=RADIUS_BAND_KPC.1).contains(&r),
                "{id}: {r} kpc at {beta}"
            );
        }
    }
    // Scutum-Centaurus winds inward past 3 kpc before 90 degrees beyond its fit, so its
    // extrapolation stops there; Perseus runs the full 90 degrees each way.
    let sct = spiral("reid-2019-scutum-centaurus-arm-extrapolated");
    let last = *sct.azimuths().last().unwrap();
    assert!(last < 104.0 + EXTRAPOLATION_DEGREES, "{last}");
    assert!(sct.radius_kpc(last + 1.0) < RADIUS_BAND_KPC.0);
    let per = spiral("reid-2019-perseus-arm-extrapolated").azimuths();
    assert_eq!((per[0], *per.last().unwrap()), (-113.0, 205.0));
}

#[test]
fn the_masers_an_arm_was_fitted_to_lie_on_it_in_the_galactic_frame() {
    // Two Perseus-arm masers of Reid et al. 2019's Table 1 (VizieR J/ApJ/885/131, as recorded
    // on 2026-11-10): NGC 7538 (G111.54+00.77, 0.378 mas), near the Sun, and G031.24-00.11
    // (0.076 mas), on the far side of the Galaxy. Each lies within the arm's width of the path;
    // mirrored about the Sun-centre line (the other sense of azimuth), the path misses them by
    // more than a kiloparsec.
    let path = spiral("reid-2019-perseus-arm").path_galactic_kpc();
    let mirrored: Vec<Vec3> = path.iter().map(|p| [p[0], -p[1], p[2]]).collect();
    for (name, l, b, mas) in [
        ("NGC 7538", 111.54, 0.77, 0.378),
        ("G031.24-00.11", 31.24, -0.11, 0.076),
    ] {
        let at = galactic(l, b, mas);
        assert!(nearest(&path, at) < 0.35, "{name}: {}", nearest(&path, at));
        assert!(nearest(&mirrored, at) > 1.0, "{name} mirrored");
    }
    // The plane lies the Sun's 5.5 pc below it.
    assert!(path.iter().all(|p| (p[2] + 0.0055).abs() < 1e-12));
}

#[test]
fn a_spiral_arms_path_is_placed_in_the_icrs_as_a_reconstructed_path_with_its_derivation() {
    let (release, correction) = (
        Release::planck_2018(),
        CmbCorrection::new(DipoleModel::Planck2018),
    );
    let p = Placing {
        release: &release,
        correction: &correction,
        dipole: "planck2018".to_owned(),
    };
    let (_, perseus) = ARMS
        .iter()
        .find(|(i, _)| i == "reid-2019-perseus-arm")
        .unwrap();
    let placed = placed_shape(perseus, None, &p).unwrap();
    assert_eq!(placed.shape.frame, Frame::Icrs);
    assert_eq!(placed.shape.unit, Unit::Parsec, "the Galaxy tracers' unit");
    let Geometry::Path { points } = &placed.shape.geometry else {
        panic!("{:?}", placed.shape.geometry)
    };
    // Turned back to the galactic frame, the path is the one computed there, in parsecs.
    let galactic = spiral("reid-2019-perseus-arm").path_galactic_kpc();
    let back = between(Frame::Icrs, Frame::Galactic);
    assert_eq!(points.len(), galactic.len());
    for (icrs, g) in points.iter().zip(&galactic) {
        let b = back.apply(*icrs);
        assert!(
            (0..3).all(|i| (b[i] - g[i] * 1_000.0).abs() < 1e-6),
            "{b:?} {g:?}"
        );
    }
    let d = &placed.derivation;
    assert_eq!(d.kind_of_value, Some(KindOfValue::Reconstructed));
    assert!(
        d.distance.method.contains("ln(R / R_kink)"),
        "{}",
        d.distance.method
    );
    assert_eq!(
        d.frame_chain
            .iter()
            .map(|s| s.transformation.clone())
            .collect::<Vec<_>>(),
        vec![Transformation::ToIcrs {
            from: Frame::Galactic
        }]
    );
    // Each pointer names a cited value of the record: R0, the Sun's height and both ends of
    // the fitted range among them.
    let record = serde_json::to_value(perseus).unwrap();
    for pointer in &d.from {
        assert!(record.pointer(pointer).is_some(), "{pointer}");
    }
    for needed in ["r0", "sun_height", "fitted/0", "fitted/1", "pitch_inside"] {
        let pointer = format!("/geometry/LogSpiral/{needed}");
        assert!(d.from.contains(&pointer), "{pointer}");
    }
}

/// A spiral arm file as written, `edit` applied to it.
fn edited(edit: impl Fn(String) -> String) -> Result<Structure, String> {
    let text =
        std::fs::read_to_string(workspace().join("structures/reid-2019-norma-arm.yaml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test-arm.yaml");
    std::fs::write(&file, edit(text)).unwrap();
    Structure::read(&file).map_err(|e| e.to_string())
}

#[test]
fn a_spiral_arm_with_an_uncited_parameter_or_another_kind_status_or_domain_is_refused() {
    assert!(edited(|t| t).is_ok());
    let uncited = edited(|t| {
        t.replace(
            "cite: { paper: reid-2019, table: \"2\", row: \"Norma\", column: \"R_kink\", cell: \"4.46 ± 0.19\" }",
            "",
        )
        .replace(
            "uncertainty: { form: symmetric, sigma: 0.19 },\n",
            "uncertainty: { form: symmetric, sigma: 0.19 } }\n",
        )
    })
    .unwrap_err();
    assert!(uncited.contains("4.46"), "{uncited}");
    let other_kind = edited(|t| t.replace("kind: spiral_arm\n", "kind: filament\n")).unwrap_err();
    assert!(other_kind.contains("log spiral"), "{other_kind}");
    let extent = edited(|t| t.replace("extent: fitted", "extent: extrapolated")).unwrap_err();
    assert!(extent.contains("spiral_arm_extrapolation"), "{extent}");
    let status = edited(|t| t.replace("status: model", "status: observed")).unwrap_err();
    assert!(status.contains("model"), "{status}");
    let domain = edited(|t| t.replace("scale_domain: galaxy_tracers", "scale_domain: cosmic_web"))
        .unwrap_err();
    assert!(domain.contains("Galaxy tracers"), "{domain}");
    let range =
        edited(|t| t.replace("value: 5, unit: degree", "value: 60, unit: degree")).unwrap_err();
    assert!(range.contains("fitted range"), "{range}");
}

#[test]
fn each_extrapolation_takes_every_cited_parameter_of_its_fitted_arm() {
    // The two files of an arm are written apart; a correction to one alone would part the
    // extrapolation from its arm.
    for (id, s) in ARMS.iter().filter(|(id, _)| !id.ends_with("-extrapolated")) {
        let fitted = spiral(id);
        let twin = spiral(&format!("{id}-extrapolated"));
        assert_eq!(fitted.scalars(), twin.scalars(), "{id}");
        assert_eq!(fitted.pairs(), twin.pairs(), "{id}");
        let name = s.name.replace(" (Reid 2019 model)", "");
        let twin_name = &ARMS
            .iter()
            .find(|(i, _)| *i == format!("{id}-extrapolated"))
            .unwrap()
            .1
            .name;
        assert!(
            twin_name.starts_with(&format!("{name} extrapolated")),
            "{twin_name}"
        );
        assert_eq!(fitted.extent, Extent::Fitted);
        assert_eq!(twin.extent, Extent::Extrapolated);
    }
}
