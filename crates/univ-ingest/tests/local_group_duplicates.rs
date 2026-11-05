//! Local Group galaxies that five catalogues list, on real recorded rows: NGC 55 in
//! `McConnachie` 2012, UNGC, Cosmicflows-4, 2MRS and 2M++, and the SMC in the first three. The
//! two catalogues' centres of one dwarf lie arcminutes apart, beyond the pair tolerance of the
//! galaxy surveys, yet they are one body by the name both publish; each galaxy is one object.
//! The pair rules for nearby galaxies and the Cosmicflows-4 velocities that are no velocity are
//! tested on SIMULATED entries and real rows.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use common::{entry, normalized_file};
use simulated::{designation, matching_rules, offset, ranking_rules, with_cz};
use univ_ingest::matching::match_entries;
use univ_ingest::ranking::rank_all;
use univ_model::derive::Release;
use univ_model::entry::CatalogueEntry;
use univ_model::object::{MeasurementRef, Object};

fn recorded() -> Vec<CatalogueEntry> {
    let mut entries = Vec::new();
    for (id, file) in [
        ("mcconnachie-2012", "mcconnachie-2012-duplicates.tsv"),
        (
            "updated-nearby-galaxies",
            "updated-nearby-galaxies-duplicates.tsv",
        ),
        ("cosmicflows-4-galaxies", "cosmicflows-4-galaxies-local.tsv"),
        ("twomrs", "twomrs-ngc55.tsv"),
        ("twompp-catalogue", "twompp-catalogue-ngc55.tsv"),
    ] {
        entries.extend(normalized_file(id, file).entries);
    }
    entries
}

fn holding<'a>(objects: &'a [Box<Object>], source: &str, id: &str) -> &'a Object {
    objects
        .iter()
        .find(|o| o.entry(&designation(source, id)).is_some())
        .unwrap()
}

#[test]
fn ngc_55_and_the_smc_are_one_object_each_across_their_catalogues() {
    let mut m = match_entries(&matching_rules(), recorded());
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    // NGC 55: UNGC's centre lies 3.2 arcmin from the others, beyond the 30 arcsec of UNGC
    // against Cosmicflows-4 and 2MRS; McConnachie's lies 0.3 arcsec from them. The name both
    // publish makes McConnachie and UNGC one body, so the survey entries need only be near one
    // of its two centres.
    let ngc55 = holding(&m.objects, "mcconnachie-2012", "NGC 55");
    for (source, id) in [
        ("updated-nearby-galaxies", "NGC0055"),
        ("cosmicflows-4-galaxies", "1014"),
        ("twomrs", "00145360-3911478"),
        ("twompp-catalogue", "00145360-3911478"),
    ] {
        assert!(
            ngc55.entry(&designation(source, id)).is_some(),
            "{source} {id}: {:#?}",
            m.objects
        );
    }
    assert!(ngc55.links.is_empty(), "{:?}", ngc55.links);
    // The SMC: McConnachie's centre lies 1.8 arcmin from UNGC's and Cosmicflows-4's, beyond the
    // 60 arcsec of McConnachie against Cosmicflows-4.
    let smc = holding(&m.objects, "mcconnachie-2012", "SMC");
    assert!(
        smc.entry(&designation("updated-nearby-galaxies", "SMC"))
            .is_some()
    );
    assert!(
        smc.entry(&designation("cosmicflows-4-galaxies", "3085"))
            .is_some(),
        "{:#?}",
        m.objects
    );
    // The merge leaves the distance to the ranking: NGC 55's preferred distance is the one
    // with the smallest uncertainty, Cosmicflows-4's modulus 26.484 +- 0.022 (1.98 Mpc).
    let p = ngc55.preferred_distance.as_ref().unwrap();
    let MeasurementRef::Distance { entry, .. } = &p.measurement else {
        panic!("{p:?}")
    };
    assert_eq!(entry.source.as_str(), "cosmicflows-4-galaxies");
    assert!((p.distance_mpc - 1.98).abs() < 0.01, "{}", p.distance_mpc);
}

#[test]
fn a_cosmicflows_velocity_of_zero_or_one_measured_wrong_is_dropped() {
    let n = normalized_file("cosmicflows-4-galaxies", "cosmicflows-4-galaxies-local.tsv");
    // Andromeda XX, Vcmb 0: no velocity was measured; Andromeda XXX, Vcmb 36 against -140
    // heliocentric (Collins et al. 2013). Both keep their modulus and position.
    for pgc in ["5056920", "5065056"] {
        let e = entry(&n, pgc);
        assert!(e.redshifts.is_empty() && e.velocity.is_none(), "{e:?}");
        assert_eq!(e.distances.len(), 1);
    }
    assert_eq!(entry(&n, "3085").redshifts.len(), 1);
}

#[test]
fn nearby_galaxies_take_the_pairs_wider_tolerance_and_ungc_meets_2mpp() {
    let rules = matching_rules();
    let (ra, dec) = (56.7, 68.1);
    // IC 342 as Cosmicflows-4 and 2MRS give it, SIMULATED: 19.5 arcsec apart at cz 31 km/s,
    // within the 30 arcsec the pair gives galaxies below 1,000 km/s; the same pair at 8,000
    // km/s stays apart, beyond its 10 arcsec.
    let far = offset(ra, dec, 19.5, 0.0);
    for (cz, objects) in [(31.0, 1), (8_000.0, 2)] {
        let m = match_entries(
            &rules,
            vec![
                with_cz(simulated::entry("twomrs", "A", ra, dec), cz, 10.0),
                with_cz(
                    simulated::entry("cosmicflows-4-galaxies", "B", far.0, far.1),
                    cz,
                    10.0,
                ),
            ],
        );
        assert_eq!(m.objects.len(), objects, "cz {cz}");
    }
    // UGC 7950: UNGC against 2M++ alone, 0.4 arcsec apart, 508 against 497 km/s.
    let near = offset(ra, dec, 0.4, 0.0);
    let m = match_entries(
        &rules,
        vec![
            with_cz(
                simulated::entry("updated-nearby-galaxies", "UGC07950", ra, dec),
                508.0,
                10.0,
            ),
            with_cz(
                simulated::entry("twompp-catalogue", "12465642+5136467", near.0, near.1),
                497.0,
                10.0,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 1);
    // Pegasus dIrr, UNGC 33.4 arcsec from Cosmicflows-4: within the pair's 60 arcsec.
    let pegasus = offset(ra, dec, 33.4, 0.0);
    let m = match_entries(
        &rules,
        vec![
            with_cz(
                simulated::entry("updated-nearby-galaxies", "Pegasus", ra, dec),
                -184.0,
                10.0,
            ),
            with_cz(
                simulated::entry("cosmicflows-4-galaxies", "71538", pegasus.0, pegasus.1),
                -184.0,
                10.0,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 1);
}

#[test]
fn a_nearby_tolerance_narrower_than_the_pairs_is_refused() {
    let mut rules = matching_rules();
    let pair = rules.pairs.iter_mut().find(|p| p.nearby.is_some()).unwrap();
    pair.nearby.as_mut().unwrap().angular_tolerance_arcsec = 1.0;
    let (field, _) = rules.check().unwrap_err();
    assert!(field.ends_with(".nearby"), "{field}");
}
