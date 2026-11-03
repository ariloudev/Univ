//! The preferred measurement where an uncertainty is missing or a parallax has a zero point, on
//! SIMULATED entries under the repository's `rules/ranking.yaml`.

mod common;
mod simulated;

use common::near;
use simulated::{entry, ranking_rules, with_bare_cz, with_modulus, with_parallax};
use univ_ingest::crossmatch::place;
use univ_ingest::ranking::rank;
use univ_model::derive::{Release, Transformation};
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, Object, ObjectEntry, Resolution,
};
use univ_model::units::Unit;

fn ranked(entries: Vec<CatalogueEntry>) -> Object {
    let mut object = Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor,
            })
            .collect(),
    );
    rank(&mut object, &ranking_rules(), &Release::planck_2018());
    object
}

#[test]
fn a_spectroscopic_redshift_without_an_error_ranks_after_a_reported_one() {
    // cz = 15,000 km/s with no error published, beside a Tully-Fisher modulus of ±0.4 mag
    // (about 18 %). The peculiar velocity alone would give the redshift about 2 %, but the
    // source reported no error for it: the modulus, which has one, wins.
    let tf = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
    let object = ranked(vec![
        with_modulus(entry("twomrs", "a", 78.0, 0.0), 35.6, 0.4, tf),
        with_bare_cz(entry("sixdfgs-dr3", "g", 78.0, 0.0), 15_000.0),
    ]);
    let preferred = object.preferred_distance.unwrap();
    assert!(
        matches!(preferred.measurement, MeasurementRef::Distance { .. }),
        "{preferred:?}"
    );
    assert_eq!(preferred.decided, Decided::Uncertainty);
    let (runner_up, uncertainty) = preferred.runner_up.unwrap();
    assert!(matches!(runner_up, MeasurementRef::Redshift { .. }));
    assert_eq!(uncertainty, FractionalUncertainty::NoneReported);
}

#[test]
fn a_gaia_parallax_is_ranked_and_placed_after_its_zero_point_and_a_hipparcos_one_is_not() {
    // ϖ = 2.000 ± 0.050 mas in Gaia DR3 is 2.017 mas after the −0.017 mas zero point of
    // the rules; in Hipparcos-2 it stays 2.000.
    let release = Release::planck_2018();
    for (source, corrected) in [("gaia-dr3-bright", 2.017), ("hipparcos-2007", 2.0)] {
        let mut object = ranked(vec![with_parallax(
            entry(source, "1", 45.0, 30.0),
            2.0,
            0.05,
        )]);
        let preferred = object.preferred_distance.clone().unwrap();
        assert!(matches!(
            preferred.measurement,
            MeasurementRef::Parallax { .. }
        ));
        assert!(
            near(preferred.distance_mpc, 1e-3 / corrected, 1e-12),
            "{source}"
        );
        let total = preferred.uncertainty.total().unwrap();
        assert!(near(total, 0.05 / corrected, 1e-12), "{source}");
        let placed = place(&mut object, &release, &ranking_rules()).unwrap();
        // The derived position's native value is the parallax as published.
        assert_eq!(
            (placed.native().value, placed.native().unit),
            (2.0, Unit::Milliarcsecond)
        );
        assert!(near(placed.distance().value, 1e-3 / corrected, 1e-12));
        let zero_point = placed
            .chain()
            .iter()
            .any(|s| matches!(s.transformation, Transformation::ParallaxZeroPoint { .. }));
        assert_eq!(zero_point, source == "gaia-dr3-bright", "{source}");
        assert!(matches!(
            placed.chain()[0].transformation,
            Transformation::PublishedParallax { .. }
        ));
    }
}
