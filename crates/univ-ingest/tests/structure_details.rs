//! Each researched structure's details as a release records them (build-a-release): its placed
//! shape in the ICRS with its derivation, its lengths rescaled to the release cosmology, and the
//! test of its outline on a volume grid, on the repository's structure files and, for the
//! outline, the SIMULATED grid and catalogues of `void`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod void;

use common::workspace;
use serde_json::Value;
use univ_ingest::outlines::place;
use univ_ingest::release::{Placing, REGION_ORDER, not_drawn, placed_shape, rescaled, result_json};
use univ_ingest::structures::{Structure, read_all_named};
use univ_model::cosmology::AssumedCosmology;
use univ_model::derive::Release;
use univ_model::geometry::{Geometry, Primitive};
use univ_model::position::{Frame, between, lon_lat, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use void::{VOID, catalogue, everywhere, grid, id, structure};

fn structures() -> Vec<(String, Structure)> {
    read_all_named(&workspace().join("structures")).unwrap()
}

fn named(id: &str) -> Structure {
    structures().into_iter().find(|(i, _)| i == id).unwrap().1
}

fn placing<'a>(release: &'a Release, correction: &'a CmbCorrection) -> Placing<'a> {
    Placing {
        release,
        correction,
        dipole: "planck2018".to_owned(),
    }
}

#[test]
fn a_sky_box_becomes_the_healpix_cells_whose_centres_lie_in_it() {
    let (release, correction) = (
        Release::planck_2018(),
        CmbCorrection::new(DipoleModel::Planck2018),
    );
    // The Hercules supercluster's box is published in FK4 B1950; the CfA2 Great Wall's, the
    // largest of the files, in equatorial coordinates of unstated equinox.
    for (id, most) in [("hercules-supercluster", 60), ("cfa2-great-wall", 8_000)] {
        let s = named(id);
        let Some(univ_ingest::structures::StructureGeometry::SkyRegion(b)) = &s.geometry else {
            panic!("{id} is a sky region")
        };
        let placed = placed_shape(&s, None, &placing(&release, &correction)).unwrap();
        assert_eq!(
            (placed.shape.frame, placed.shape.unit),
            (Frame::Icrs, Unit::Megaparsec)
        );
        let Geometry::SkyRegion {
            order,
            cells,
            near,
            far,
        } = &placed.shape.geometry
        else {
            panic!("{id}: {:?}", placed.shape.geometry)
        };
        assert_eq!(*order, REGION_ORDER);
        assert!(
            !cells.is_empty() && cells.len() < most,
            "{id}: {} cells",
            cells.len()
        );
        assert!(cells.windows(2).all(|w| w[0] < w[1]));
        assert!(0.0 < *near && near < far, "{id}: {near} {far}");
        let lon0 = b.lon[0].value();
        let span = (b.lon[1].value() - lon0).rem_euclid(360.0);
        let into_box = between(Frame::Icrs, b.frame);
        for &c in cells {
            let (ra, dec) = cdshealpix::nested::center(*order, c);
            let (lon, lat) =
                lon_lat(into_box.apply(unit_vector(ra.to_degrees(), dec.to_degrees())));
            assert!(
                (lon - lon0).rem_euclid(360.0) <= span,
                "{id}: cell {c} at lon {lon}"
            );
            assert!(
                (b.lat[0].value()..=b.lat[1].value()).contains(&lat),
                "{id}: {c} at {lat}"
            );
        }
        let chain = placed.derivation["frame_chain"].as_array().unwrap();
        assert!(
            chain.last().unwrap().as_str().unwrap().ends_with("-> icrs"),
            "{chain:?}"
        );
    }
}

/// Whether every pointer of `list` names a value of `record`.
fn resolves(record: &Value, list: &Value) -> bool {
    list.as_array()
        .unwrap()
        .iter()
        .all(|p| record.pointer(p.as_str().unwrap()).is_some())
}

#[test]
fn every_structure_names_the_cited_values_its_shape_and_rescaled_lengths_come_from() {
    let (release, correction) = (
        Release::planck_2018(),
        CmbCorrection::new(DipoleModel::Planck2018),
    );
    let mut shapes = 0;
    for (id, s) in structures() {
        let record = serde_json::to_value(&s).unwrap();
        // Member lists are placed by the cross-match, which this test does not run.
        if let Ok(placed) = placed_shape(&s, None, &placing(&release, &correction)) {
            shapes += 1;
            let d = &placed.derivation;
            assert!(resolves(&record, &d["from"]), "{id}: {d}");
            assert_eq!(d["cosmology"], "release", "{id}");
            assert!(d["distance"]["method"].is_string(), "{id}: {d}");
        }
        if let Some(items) = rescaled(&s, &release).unwrap() {
            for item in items.as_array().unwrap() {
                let at = record.pointer(item["value"].as_str().unwrap());
                assert!(at.is_some(), "{id}: {item}");
            }
        }
    }
    assert!(shapes >= 15, "{shapes}");
}

#[test]
fn a_radius_published_for_h0_50_is_rescaled_by_50_over_67_66() {
    let release = Release::planck_2018();
    let bootes = named("bootes-void");
    let items = rescaled(&bootes, &release).unwrap().unwrap();
    let radius = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["value"] == "/geometry/Primitive/size/radius")
        .unwrap();
    let factor = release
        .hubble_rescaling(AssumedCosmology::hubble(50.0))
        .unwrap()
        .factor;
    assert!((factor - 50.0 / 67.66).abs() < 1e-12);
    assert_eq!(radius["factor"].as_f64(), Some(factor));
    assert_eq!(radius["method"], "hubble_ratio");
    assert_eq!(radius["native"]["value"], 63.0);
    assert_eq!(radius["native"]["hubble_constant"], 50.0);
    assert_eq!(radius["native"]["assumed"]["stated"], "H0 = 50 km/s/Mpc");
    assert_eq!(radius["rescaled"]["unit"], "megaparsec");
    let rescaled = radius["rescaled"]["value"].as_f64().unwrap();
    assert!((rescaled - 63.0 * 50.0 / 67.66).abs() < 1e-9, "{rescaled}");
    // The sphere placed is of that radius.
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let placed = placed_shape(&bootes, None, &placing(&release, &correction)).unwrap();
    let Geometry::Primitive {
        primitive: Primitive::Sphere { radius, .. },
    } = placed.shape.geometry
    else {
        panic!("{:?}", placed.shape.geometry)
    };
    assert!((radius - rescaled).abs() < 1e-9);
}

#[test]
fn an_outline_test_is_recorded_with_its_verdict_numbers_and_checks() {
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    let crowded = catalogue("simulated-crowded", 2, 4, 0.0, everywhere);
    let admitted = place(
        &structure(VOID, "simulated-full", ""),
        &grid(),
        &[(id("simulated-full"), &full[..])],
    )
    .unwrap();
    let column = result_json(&admitted);
    assert_eq!(column["verdict"], "admitted");
    assert_eq!(column["reason"], Value::Null);
    assert_eq!(
        (column["cells"].as_u64(), column["touches_edge"].as_bool()),
        (Some(19), Some(false))
    );
    assert_eq!(column["grid"], "simulated-density");
    assert_eq!(column["field"], "density");
    assert_eq!(column["threshold"]["chosen"]["chosen"], -1.0);
    assert_eq!(column["test"]["ratio"], 0.85);
    assert_eq!(column["checks"][0]["source"], "simulated-full");
    assert_eq!(column["checks"][0]["passes"], true);
    let s = structure(VOID, "simulated-crowded", "");
    let refused = place(&s, &grid(), &[(id("simulated-crowded"), &crowded[..])]).unwrap();
    let column = result_json(&refused);
    assert_eq!(column["verdict"], "refused");
    assert!(
        column["reason"]
            .as_str()
            .unwrap()
            .contains("simulated-crowded holds"),
        "{column}"
    );
    let record = s.outline.as_ref().unwrap();
    let missing = not_drawn(record, "its volume grid is not among this release's grids");
    assert_eq!(missing["verdict"], "refused");
    assert_eq!(missing["cells"], 0);
}
