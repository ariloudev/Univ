//! A researched structure's members that no catalogue object carries, placed from the structure
//! file itself: each at its own cited sky position and redshift, the heliocentric redshift moved
//! to the CMB frame and made a comoving distance under the release cosmology, recorded as a
//! tracer with the values that placed it, so the member list is placed whole and bounded. The
//! real structure files of the repository, with no matched object.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use common::workspace;
use univ_ingest::crossmatch::{FilePlacer, MemberPlacement, NameIndex, resolve_all};
use univ_ingest::structures::{Structure, read_all_named};
use univ_model::derive::Release;
use univ_model::geometry::{Geometry, Member};
use univ_model::position::{Frame, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel, RedshiftFrame};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

/// The member lists of the repository's structures, resolved against no object.
fn placed() -> (Vec<(String, Structure)>, Vec<MemberPlacement>) {
    let named = read_all_named(&workspace().join("structures")).unwrap();
    let structures: Vec<Structure> = named.iter().map(|(_, s)| s.clone()).collect();
    let release = Release::planck_2018();
    let correction =
        univ_model::redshift::CmbCorrection::new(univ_model::redshift::DipoleModel::Planck2018);
    let placer = FilePlacer::new(&release, &correction);
    let lists = resolve_all(&structures, &NameIndex::new(&[]), 60.0, Some(&placer));
    (named, lists)
}

fn list<'a>(lists: &'a [MemberPlacement], name: &str) -> &'a MemberPlacement {
    lists.iter().find(|l| l.structure == name).unwrap()
}

#[test]
fn a_member_no_object_carries_is_placed_from_its_own_cited_position_and_redshift() {
    let (_, lists) = placed();
    let lqg = list(&lists, "Huge-LQG");
    assert!(lqg.unplaced.is_empty(), "{:?}", lqg.unplaced);
    assert_eq!(lqg.from_file.len(), 73);
    let shape = lqg.shape.as_ref().unwrap();
    let Geometry::Members { members } = &shape.geometry else {
        panic!("{shape:?}")
    };
    let Member::Tracer { at, kind } = &members[0] else {
        panic!("{:?}", members[0])
    };
    assert_eq!(kind, "quasar");
    // Clowes et al. 2013, Table 1, row 1: SDSS J104139.15+143530.2, 10:41:39.15 +14:35:30.2,
    // z = 1.2164 (heliocentric).
    let first = &lqg.from_file[0];
    assert_eq!(first.member, "SDSS J104139.15+143530.2");
    assert_eq!(
        (first.redshift, first.kind),
        (1.2164, KindOfValue::Measured)
    );
    assert!(
        first.redshift_from.contains("\"z\""),
        "{}",
        first.redshift_from
    );
    let toward = unit_vector(
        (10.0 + 41.0 / 60.0 + 39.15 / 3_600.0) * 15.0,
        14.0 + 35.0 / 60.0 + 30.2 / 3_600.0,
    );
    let z = Quantity::new(1.2164, Unit::Dimensionless, Uncertainty::NoneReported).unwrap();
    let cmb = CmbCorrection::new(DipoleModel::Planck2018)
        .apply(z, RedshiftFrame::Heliocentric, toward)
        .unwrap();
    assert!((first.cmb_redshift - cmb.value.value).abs() < 1e-12);
    assert!(
        (first.cmb_redshift - 1.2164).abs() < 0.003 * 2.2164,
        "the dipole moves it at most 370 km/s"
    );
    let d = first.distance_mpc;
    assert!(
        (3_500.0..4_200.0).contains(&d),
        "comoving distance at z = 1.22 under Planck 2018: {d}"
    );
    for k in 0..3 {
        assert!((at[k] - toward[k] * d).abs() < 1e-6, "{at:?}");
    }
    assert!(lqg.bounds.is_some() && lqg.point.is_none());
}

#[test]
fn a_member_table_without_redshifts_takes_the_structures_cited_redshift_as_an_estimate() {
    let (named, lists) = placed();
    let arc = list(&lists, "Giant Arc");
    assert!(arc.unplaced.is_empty() && arc.shape.is_some());
    assert_eq!(arc.from_file.len(), 55);
    for m in &arc.from_file {
        assert_eq!(m.redshift, 0.802, "Lopez et al. 2022: z = 0.802 ± 0.060");
        assert_eq!(m.kind, KindOfValue::Estimated);
        assert_eq!(m.redshift_from, "the structure's value central_redshift");
    }
    let ring = list(&lists, "Big Ring");
    assert!(
        ring.from_file
            .iter()
            .all(|m| (m.redshift - 0.802).abs() < 1e-12)
    );
    // The GRB ring's table gives its paper's distances too; the release cosmology places it.
    let grb = list(&lists, "Giant GRB Ring");
    assert_eq!(grb.from_file.len(), 9);
    let (_, file) = named.iter().find(|(id, _)| id == "giant-grb-ring").unwrap();
    assert_eq!(file.name, "Giant GRB Ring");
    let shape = grb.shape.as_ref().unwrap();
    assert!(shape.bounds_in(Frame::Icrs).is_some());
    // Every member list of the repository is now placed whole.
    for l in &lists {
        assert!(l.shape.is_some(), "{}: {:?}", l.structure, l.unplaced);
    }
}

#[test]
fn a_member_placed_at_its_structure_s_redshift_slice_carries_the_slice_s_width() {
    // Lopez et al. 2022 and 2024: the Giant Arc and the Big Ring lie in the slice z = 0.802 ±
    // 0.060. Each absorber placed at 0.802 carries ±0.060 as a stated interval, not a sigma,
    // and the comoving distances of the slice's two ends as the bounds of its distance, with no
    // peculiar-velocity term added to them.
    let (_, lists) = placed();
    let release = Release::planck_2018();
    let (low, high) = (
        release.cosmology().comoving_distance(0.742).unwrap(),
        release.cosmology().comoving_distance(0.862).unwrap(),
    );
    for name in ["Giant Arc", "Big Ring"] {
        let l = list(&lists, name);
        assert!(!l.from_file.is_empty(), "{name}");
        for m in &l.from_file {
            assert_eq!(
                m.redshift_uncertainty,
                Uncertainty::Interval {
                    minus: 0.060,
                    plus: 0.060
                },
                "{name}"
            );
            let [near, far] = m.distance_range_mpc.unwrap();
            assert!(near < m.distance_mpc && m.distance_mpc < far);
            // The ends are the slice's own, in the CMB frame toward the absorber.
            let cmb = m.cmb_redshift - m.redshift;
            let end = |z: f64| release.cosmology().comoving_distance(z + cmb).unwrap();
            assert!((near - end(0.742)).abs() < 0.002 * near, "{near}");
            assert!((far - end(0.862)).abs() < 0.002 * far, "{far}");
            // The CMB-frame shift toward each absorber moves the ends by well under 1%.
            assert!((near - low).abs() < 0.01 * low && (far - high).abs() < 0.01 * high);
        }
    }
}

#[test]
fn saraswati_s_cluster_redshifts_are_estimated_as_some_are_photometric() {
    // Bagchi et al. 2017 used photometric redshifts for the clusters whose BCG spectra are not
    // available, and Table 1 does not say which: none is called measured.
    let (_, lists) = placed();
    let l = list(&lists, "Saraswati Supercluster");
    assert_eq!(l.from_file.len(), 48);
    for m in &l.from_file {
        assert_eq!(m.kind, KindOfValue::Estimated, "{}", m.member);
        assert_eq!(m.redshift_uncertainty, Uncertainty::NoneReported);
        // A redshift without an error still has the peculiar-velocity term in its distance.
        let [near, far] = m.distance_range_mpc.unwrap();
        let term = 300.0 * (1.0 + m.cmb_redshift) / 299_792.458;
        let slope = (far - near) / (2.0 * term);
        assert!(
            near < m.distance_mpc && m.distance_mpc < far,
            "{}",
            m.member
        );
        assert!(slope > 0.0 && (far - m.distance_mpc) < 0.01 * m.distance_mpc);
    }
}

/// A cluster table that gives each member's supergalactic X, Y and Z, as Tully 1987's Table 1
/// does, beside its redshift.
const SUPERGALACTIC: &str = r#"
name: Test Supercluster
kind: supercluster
status: observed
scale_domain: cosmic_web
papers:
  - id: tully-1987
    reference: "Tully 1987, ApJ 323, 1"
    title: "More about clustering on a scale of 0.1 c"
    doi: 10.1086/165803
    read: "ADS scan"
geometry:
  kind: members
  members:
    frame: supergalactic
    tracer: galaxy cluster
    source: { paper: tully-1987, table: "1" }
    columns:
      - { name: redshift, header: "z", unit: dimensionless, kind: measured, uncertainty: { form: none_reported } }
      - { name: x, header: "SGX", unit: megaparsec, kind: inferred, uncertainty: { form: none_reported }, assumes: "h75^-1 Mpc: H0 = 75 km/s/Mpc" }
      - { name: y, header: "SGY", unit: megaparsec, kind: inferred, uncertainty: { form: none_reported }, assumes: "h75^-1 Mpc: H0 = 75 km/s/Mpc" }
      - { name: z, header: "SGZ", unit: megaparsec, kind: inferred, uncertainty: { form: none_reported }, assumes: "h75^-1 Mpc: H0 = 75 km/s/Mpc" }
    rows:
      - ["A1", 0.03, 0, 100, 0]
notes: "A test structure."
"#;

#[test]
fn a_member_given_by_cartesian_coordinates_lies_in_their_direction_at_its_redshift() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test-supercluster.yaml");
    std::fs::write(&file, SUPERGALACTIC).unwrap();
    let s = Structure::read(&file).unwrap();
    let release = Release::planck_2018();
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let placer = FilePlacer::new(&release, &correction);
    let lists = resolve_all(&[s], &NameIndex::new(&[]), 60.0, Some(&placer));
    let list = &lists[0];
    assert!(list.unplaced.is_empty(), "{:?}", list.unplaced);
    let Some(Geometry::Members { members }) = list.shape.as_ref().map(|s| &s.geometry) else {
        panic!("placed whole")
    };
    let Member::Tracer { at, .. } = &members[0] else {
        panic!("a tracer")
    };
    // Along supergalactic +Y, at the comoving distance of its CMB-frame redshift.
    let toward =
        univ_model::position::between(Frame::Supergalactic, Frame::Icrs).apply([0.0, 1.0, 0.0]);
    let record = &list.from_file[0];
    let distance = univ_model::position::norm(*at);
    assert!((distance - record.distance_mpc).abs() < 1e-9);
    for (got, want) in at.iter().zip(toward) {
        assert!(
            (got / distance - want).abs() < 1e-12,
            "{at:?} vs {toward:?}"
        );
    }
    // A table with both a sky position and Cartesian coordinates is refused.
    let both = SUPERGALACTIC.replace(
        "    columns:\n",
        "    columns:\n      - { name: lon, header: \"SGL\", unit: degree, kind: measured, uncertainty: { form: none_reported } }\n",
    );
    std::fs::write(&file, both).unwrap();
    assert!(Structure::read(&file).is_err());
    // Coordinates give only the direction, so a table without redshifts is refused, as are
    // coordinates in two units.
    let no_redshift = SUPERGALACTIC.replace(
        "      - { name: redshift, header: \"z\", unit: dimensionless, kind: measured, uncertainty: { form: none_reported } }\n",
        "",
    ).replace("[\"A1\", 0.03, ", "[\"A1\", ");
    std::fs::write(&file, no_redshift).unwrap();
    assert!(Structure::read(&file).is_err());
    let two_units = SUPERGALACTIC.replacen(
        "{ name: z, header: \"SGZ\", unit: megaparsec",
        "{ name: z, header: \"SGZ\", unit: megaparsec_over_h",
        1,
    );
    std::fs::write(&file, two_units).unwrap();
    assert!(Structure::read(&file).is_err());
}
