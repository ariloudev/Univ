//! The Cosmicflows-4 galaxies table (the 19th source) on its real recorded answer: M87 and six
//! more members of the Virgo group dominated by M49 (1PGC 41220), with M31 and Centaurus A, and
//! three real 2MRS rows of the same Virgo galaxies. The one group entry below is SIMULATED.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use common::{entry, normalized_file, normalized_fixture};
use simulated::{designation, matching_rules, ranking_rules, with_cz, with_modulus};
use univ_ingest::crossmatch::place;
use univ_ingest::matching::match_entries;
use univ_ingest::normalize::Normalized;
use univ_ingest::ranking::rank_all;
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::derive::Release;
use univ_model::derive::Transformation;
use univ_model::distance::{DistanceKind, Measure, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{LineOfSight, MeasurementRef, Object, RelationKind, Resolution};
use univ_model::redshift::{CmbCorrection, DipoleModel, RedshiftFrame};
use univ_model::units::Unit;

const M87: &str = "41361";

fn twomrs_virgo() -> Normalized {
    normalized_file("twomrs", "twomrs-virgo.tsv")
}

fn galaxies() -> Normalized {
    normalized_fixture("cosmicflows-4-galaxies")
}

fn object_of<'a>(objects: &'a [Object], source: &str, id: &str) -> &'a Object {
    objects
        .iter()
        .find(|o| o.entry(&designation(source, id)).is_some())
        .unwrap()
}

#[test]
fn every_row_reads_with_its_own_modulus_group_and_cmb_velocity() {
    let n = galaxies();
    assert_eq!(
        (n.entries.len(), n.report.left_out.len()),
        (9, 0),
        "{:?}",
        n.report
    );
    let m87 = entry(&n, M87);
    assert_eq!(m87.group_id.as_deref(), Some("41220"));
    assert_eq!(m87.distances.len(), 1, "DM alone is taken");
    let d = &m87.distances[0];
    assert_eq!(d.native.value, 31.134);
    assert_eq!(d.measure, Measure::Luminosity);
    // DM rests on SBF here, one of the five methods: a secondary indicator.
    assert_eq!(d.kind, DistanceKind::SecondaryIndicator);
    assert!(matches!(&d.method, Method::Mixed(m) if m.len() == 8));
    let mpc = d.normalized.to(Unit::Megaparsec).unwrap().value;
    assert!((mpc - 16.86).abs() < 0.01, "{mpc}");
    assert_eq!(m87.velocity.as_ref().unwrap().normalized.value, 1608.0);
    // Vcmb is also its redshift, in the CMB frame as published: no dipole is applied again.
    let [z] = &m87.redshifts[..] else {
        panic!("{:?}", m87.redshifts)
    };
    assert_eq!(z.cmb.published, RedshiftFrame::Cmb);
    assert_eq!(z.cmb.value.value, 1608.0 / SPEED_OF_LIGHT);
    assert_eq!(z.velocity.unwrap().value, 1608.0);
    // Centaurus A: 27.804 ± 0.038 mag, 3.64 Mpc.
    let cen_a = entry(&n, "46957");
    let mpc = cen_a.distances[0]
        .normalized
        .to(Unit::Megaparsec)
        .unwrap()
        .value;
    assert!((mpc - 3.64).abs() < 0.01, "{mpc}");
}

/// A SIMULATED row of the Cosmicflows-4 groups table for the Virgo group, 1PGC 41220, at the
/// group centre of Kourkchi & Tully 2017.
fn virgo_group() -> CatalogueEntry {
    simulated::entry("cosmicflows-4", "41220", 187.756, 12.406)
}

fn matched() -> Vec<Object> {
    let mut entries = galaxies().entries;
    entries.extend(twomrs_virgo().entries);
    entries.push(virgo_group());
    let mut m = match_entries(&matching_rules(), entries);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    m.objects
}

#[test]
fn m87_merges_with_its_2mrs_entry_by_position_and_heliocentric_velocity() {
    // Cosmicflows-4 gives Vcmb = 1608 km/s, 2MRS cz = 1307 km/s: 301 km/s apart, beyond the
    // 200 km/s tolerance as published, 30 km/s apart once Vcmb is moved back to the
    // heliocentric frame along M87's direction.
    let objects = matched();
    let m87 = object_of(&objects, "cosmicflows-4-galaxies", M87);
    let twomrs = m87
        .entries
        .iter()
        .find(|e| e.entry.designation.source.as_str() == "twomrs")
        .unwrap();
    let Resolution::Match(record) = &twomrs.resolved_by else {
        panic!("{:?}", twomrs.resolved_by)
    };
    assert!(record.separation_arcsec < 1.0, "{record:?}");
    let Some(LineOfSight::Velocity { delta_km_s }) = record.difference else {
        panic!("{record:?}")
    };
    assert!(delta_km_s < 60.0, "{delta_km_s}");
    // M49 and M84 merge with theirs too.
    for id in ["41220", "40455"] {
        assert_eq!(
            object_of(&objects, "cosmicflows-4-galaxies", id)
                .entries
                .len(),
            2
        );
    }
}

#[test]
fn m87_is_placed_by_its_cosmicflows_4_distance_not_its_redshift() {
    let objects = matched();
    let p = object_of(&objects, "cosmicflows-4-galaxies", M87)
        .preferred_distance
        .as_ref()
        .unwrap();
    assert_eq!(
        p.measurement,
        MeasurementRef::Distance {
            entry: designation("cosmicflows-4-galaxies", M87),
            index: 0
        }
    );
    assert!(
        p.distance_mpc > 16.0 && p.distance_mpc < 17.0,
        "{}",
        p.distance_mpc
    );
    let (beaten, _) = p.runner_up.as_ref().unwrap();
    assert!(matches!(beaten, MeasurementRef::Redshift { .. }));
}

#[test]
fn each_galaxy_is_related_to_its_group_without_a_merge_or_a_loan() {
    let objects = matched();
    let group = object_of(&objects, "cosmicflows-4", "41220");
    assert_eq!(group.entries.len(), 1, "a group merges with no galaxy");
    let members = group
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Member)
        .count();
    assert_eq!(members, 7, "{:?}", group.relations);
    let dominant: Vec<_> = group
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::DominantMember)
        .map(|r| r.other.clone())
        .collect();
    assert_eq!(dominant, [designation("cosmicflows-4-galaxies", "41220")]);
    let m87 = object_of(&objects, "cosmicflows-4-galaxies", M87);
    let of = m87
        .relations
        .iter()
        .find(|r| r.kind == RelationKind::MemberOf)
        .unwrap();
    assert_eq!(
        (of.other.clone(), of.scheme.as_str(), of.identifier.as_str()),
        (designation("cosmicflows-4", "41220"), "pgc", "41220")
    );
    // Nothing is lent: the group, which carries no distance here, has none.
    assert!(group.preferred_distance.is_none());
    assert!(m87.entry(&designation("cosmicflows-4", "41220")).is_none());
    // M31 and Centaurus A are their own groups' galaxies, whose group rows are not here.
    let cen_a = object_of(&objects, "cosmicflows-4-galaxies", "46957");
    assert_eq!(cen_a.relations, []);
}

#[test]
fn a_cosmicflows_4_galaxy_is_placed_at_its_luminosity_distance_over_one_plus_its_cmb_redshift() {
    // DM is a luminosity distance: M87's 31.134 mag is D_L = 16.858 Mpc, and its Vcmb of
    // 1608 km/s makes it comoving, D_L / (1 + z), once, with no second dipole step.
    let mut objects = matched();
    let release = Release::planck_2018();
    let at = objects
        .iter()
        .position(|o| {
            o.entry(&designation("cosmicflows-4-galaxies", M87))
                .is_some()
        })
        .unwrap();
    let placed = place(&mut objects[at], &release, &ranking_rules()).unwrap();
    let d_l = 10f64.powf(31.134 / 5.0 + 1.0) * 1e-6;
    let z = 1608.0 / SPEED_OF_LIGHT;
    let d = placed.distance().value;
    assert!(
        (d - d_l / (1.0 + z)).abs() < 1e-9,
        "{d} against {}",
        d_l / (1.0 + z)
    );
    assert!(placed.chain().iter().any(|s| matches!(
        s.transformation,
        Transformation::LuminosityToComoving { cmb_redshift, .. } if (cmb_redshift - z).abs() < 1e-15
    )));
    // A group and its galaxy are made comoving alike: a SIMULATED Virgo group row with M87's
    // modulus and a heliocentric velocity that the dipole takes to the same CMB-frame value is
    // placed at the same distance.
    let direction = placed.direction().icrs().unit_vector();
    let heliocentric = CmbCorrection::new(DipoleModel::Planck2018).heliocentric_of(z, direction);
    let group = with_cz(
        with_modulus(
            virgo_group(),
            31.134,
            0.04,
            (DistanceKind::SecondaryIndicator, Method::TullyFisher),
        ),
        heliocentric * SPEED_OF_LIGHT,
        0.0,
    );
    let mut alone = Object::new(vec![univ_model::object::ObjectEntry {
        entry: group,
        resolved_by: Resolution::Anchor,
    }]);
    univ_ingest::ranking::rank(&mut alone, &ranking_rules(), &release);
    assert!(matches!(
        alone.preferred_distance.as_ref().unwrap().measurement,
        MeasurementRef::Distance { .. }
    ));
    let group_placed = place(&mut alone, &release, &ranking_rules()).unwrap();
    let g = group_placed.distance().value;
    assert!((g - d).abs() < 1e-6 * d, "group {g} against galaxy {d}");
}
