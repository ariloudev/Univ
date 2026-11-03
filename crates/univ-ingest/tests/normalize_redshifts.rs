//! Each redshift is kept as its source published it, and its CMB-frame value derived with the
//! Planck 2018 dipole, the derivation recorded with it (F-4).

mod common;

use common::{entry, every_table, normalized_fixture};
use univ_model::redshift::DipoleModel;
use univ_model::value::Uncertainty;

#[test]
fn a_heliocentric_redshift_is_kept_as_measured() {
    let sdss = normalized_fixture("sdss-dr17-galaxies");
    // Row 2: 375050326987794432, z 0.08733442 ± 2.054854E-05.
    let r = &entry(&sdss, "375050326987794432").redshifts[0];
    assert_eq!(r.cmb.heliocentric.value, 0.087_334_42);
    assert_eq!(
        r.cmb.heliocentric.uncertainty,
        Uncertainty::Symmetric {
            sigma: 2.054_854e-5
        }
    );
    assert_eq!(r.velocity, None);
}

#[test]
fn the_cmb_redshift_uses_the_planck_2018_dipole() {
    for (_, n) in every_table() {
        for r in n.entries.iter().flat_map(|e| &e.redshifts) {
            assert_eq!(r.cmb.dipole, DipoleModel::Planck2018);
        }
    }
    let dipole = DipoleModel::Planck2018.dipole();
    assert_eq!(
        (dipole.speed, dipole.l, dipole.b),
        (369.82, 264.021, 48.253)
    );
}

#[test]
fn the_cmb_derivation_is_recorded_with_the_redshift_it_was_derived_from() {
    let twomrs = normalized_fixture("twomrs");
    // Row 3: 09553318+6903549, cz -34 ± 4 km/s.
    let r = &entry(&twomrs, "09553318+6903549").redshifts[0];
    assert_eq!(r.velocity.unwrap().value, -34.0);
    assert_eq!(r.cmb.heliocentric.value, -34.0 / 299_792.458);
    assert_ne!(r.cmb.value.value, r.cmb.heliocentric.value);
    let yaml = serde_norway::to_string(r).unwrap();
    assert!(
        yaml.contains("heliocentric:") && yaml.contains("dipole: planck2018"),
        "{yaml}"
    );
}
