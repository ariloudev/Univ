//! Each source's coverage as a release records it (build-a-release): the `HEALPix` cells of
//! order 5 its entries fall in, and the range of each radial measure they hold. The entries are
//! SIMULATED directions, then the recorded fixtures of real sources.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod void;

use std::f64::consts::FRAC_PI_2;

use common::{definition_of, normalized_fixture};
use univ_ingest::release::{COVERAGE_ORDER, Coverage, cells_of, healpix_cell};
use univ_ingest::tiles::unit_of;
use univ_model::entry::{CatalogueEntry, NativePosition, Position, PositionUncertainty};
use univ_model::position::{Epoch, Frame};
use univ_model::units::Unit;

/// A SIMULATED entry at ICRS (`ra`, `dec`), degrees, with the simulated redshift of `void`.
fn at(n: usize, ra: f64, dec: f64) -> CatalogueEntry {
    let mut e = void::entry("simulated-survey", n, [10.0, 0.0, 0.0], true);
    let native = NativePosition {
        frame: Frame::Icrs,
        lon: ra,
        lat: dec,
        unit: Unit::Degree,
        epoch: Epoch::J2000,
    };
    e.position = Some(Position::new(native, PositionUncertainty::NoneReported).unwrap());
    assert_eq!(e.designation.source, void::id("simulated-survey"));
    e
}

#[test]
fn a_sources_sky_is_the_sorted_order_5_cells_of_its_entries() {
    let directions = [(200.0, -45.0), (0.0, 90.0), (10.0, 20.0), (10.001, 20.001)];
    let mut coverage = Coverage::new(Unit::Megaparsec);
    for (n, (ra, dec)) in directions.iter().enumerate() {
        coverage.add(&at(n, *ra, *dec), None);
    }
    let sky = coverage.sky().unwrap();
    assert_eq!(sky.order, 5);
    let cells = sky.cells;
    // The two entries 0.0014° apart share a cell: three cells, ascending, each the one
    // cdshealpix gives the entry's direction at order 5.
    let mut expected: Vec<u64> = directions
        .iter()
        .map(|(ra, dec)| cdshealpix::nested::hash(5, f64::to_radians(*ra), dec.to_radians()))
        .collect();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(cells, expected);
    assert_eq!(cells, cells_of(COVERAGE_ORDER, directions));
    assert!(cells.windows(2).all(|w| w[0] < w[1]));
    // The north pole lies in one of the four northern base pixels (nested indices 0 to 3 of
    // order 0): a cell of order 5 is its base pixel times 4^5 plus its place within it.
    let pole = cdshealpix::nested::hash(5, 0.0, FRAC_PI_2);
    assert_eq!(healpix_cell(5, 0.0, 90.0), pole);
    assert!(cells.contains(&pole));
    assert!(pole >> 10 < 4, "{pole}");
    assert_eq!(pole >> 10, cdshealpix::nested::hash(0, 0.0, FRAC_PI_2));
    // Every entry here has one redshift.
    let radial = coverage.radial().unwrap();
    assert_eq!(radial.len(), 1);
    assert_eq!(radial[0].measure, "redshift");
    assert_eq!(radial[0].entries, 4);
}

#[test]
fn a_source_without_entries_has_no_sky_and_no_radial_range() {
    let coverage = Coverage::new(Unit::Megaparsec);
    assert_eq!(coverage.sky(), None);
    assert_eq!(coverage.radial(), None);
}

/// The coverage of a source's recorded fixture, in the unit of its distances.
fn fixture_coverage(id: &str) -> (Vec<CatalogueEntry>, serde_json::Value) {
    let entries = normalized_fixture(id).entries;
    let mut coverage = Coverage::new(unit_of(definition_of(id).scale_domain));
    for e in &entries {
        coverage.add(e, None);
    }
    let radial = serde_json::to_value(coverage.radial().unwrap()).unwrap();
    (entries, radial)
}

/// The item of `measure` among a radial column's items.
fn item<'a>(radial: &'a serde_json::Value, measure: &str) -> &'a serde_json::Value {
    radial
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["measure"] == measure)
        .unwrap()
}

fn range(values: impl Iterator<Item = f64>) -> (f64, f64, u64) {
    values.fold((f64::INFINITY, f64::NEG_INFINITY, 0), |(lo, hi, n), v| {
        (lo.min(v), hi.max(v), n + 1)
    })
}

#[test]
fn the_radial_range_of_2mrs_is_that_of_its_redshifts_as_published() {
    let (entries, radial) = fixture_coverage("twomrs");
    // 2MRS publishes the heliocentric velocity cz: z = cz / c as normalized.
    let (lo, hi, n) = range(
        entries
            .iter()
            .filter_map(|e| e.redshifts.first())
            .map(|z| z.cmb.as_published().value),
    );
    assert!(n > 0);
    let z = item(&radial, "redshift");
    assert_eq!(z["min"], lo);
    assert_eq!(z["max"], hi);
    assert_eq!(z["entries"], n);
    assert_eq!(z["unit"], "dimensionless");
    let basis = z["basis"].as_str().unwrap();
    assert!(basis.contains("heliocentric"), "{basis}");
    assert!(basis.contains("published as the velocity cz"), "{basis}");
}

#[test]
fn the_radial_ranges_of_hipparcos_and_cosmicflows_4_are_their_parallaxes_and_distances() {
    let (entries, radial) = fixture_coverage("hipparcos-2007");
    let (lo, hi, n) = range(entries.iter().filter_map(|e| {
        e.parallax
            .map(|p| p.normalized.to(Unit::Milliarcsecond).unwrap().value)
    }));
    let p = item(&radial, "parallax");
    assert_eq!(
        (p["unit"].as_str(), p["entries"].as_u64()),
        (Some("milliarcsecond"), Some(n))
    );
    assert!((p["min"].as_f64().unwrap() - lo).abs() < 1e-9);
    assert!((p["max"].as_f64().unwrap() - hi).abs() < 1e-9);
    // Cosmicflows-4's group distances, in Mpc as normalized.
    let (entries, radial) = fixture_coverage("cosmicflows-4");
    let (lo, hi, n) = range(
        entries
            .iter()
            .flat_map(|e| &e.distances)
            .map(|d| d.normalized.to(Unit::Megaparsec).unwrap().value),
    );
    let d = item(&radial, "distance");
    assert_eq!(d["unit"], "megaparsec");
    assert_eq!((d["min"].as_f64(), d["max"].as_f64()), (Some(lo), Some(hi)));
    assert!(d["entries"].as_u64().unwrap() <= n && n > 0);
}
