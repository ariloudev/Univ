//! An uncertainty a source states once for its whole catalogue is given to each of its values as
//! an estimated uncertainty that cites where it is stated (normalize-records).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a failed unwrap or match is the test failing"
)]

mod common;

use std::fs;

use common::{definition_of, fixture_of, normalize_body};
use univ_ingest::source::{CatalogueError, SourceDefinition, read_all};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::units::Unit;
use univ_model::value::Uncertainty;

/// `id`'s definition with its first redshift's error column replaced by `catalogue`.
fn with_catalogue_error(id: &str, catalogue: CatalogueError) -> SourceDefinition {
    let mut d = definition_of(id);
    let r = &mut d.normalization.redshifts[0];
    r.error = None;
    r.catalogue_error = Some(catalogue);
    d
}

fn sigma(u: Uncertainty) -> f64 {
    let Uncertainty::Symmetric { sigma } = u else {
        panic!("{u:?}")
    };
    sigma
}

#[test]
fn an_rms_velocity_for_the_catalogue_is_each_velocitys_symmetric_error() {
    // 2dFGRS states one rms of 85 km/s for its quality 3+ redshifts (Colless et al. 2001).
    let d = with_catalogue_error(
        "sixdfgs-dr3",
        CatalogueError {
            value: 85.0,
            unit: Unit::KilometrePerSecond,
            times_one_plus_z: false,
            basis: "Colless et al. 2001, §6: an rms of 85 km/s".to_owned(),
        },
    );
    let n = normalize_body(&d, &fs::read(fixture_of(&d)).unwrap());
    assert_ne!(n.entries.len(), 0);
    for e in &n.entries {
        let r = &e.redshifts[0];
        let cz = r.velocity.unwrap();
        assert_eq!(sigma(cz.uncertainty), 85.0);
        // The dimensionless z it was read as carries σz = σv / c.
        let z = sigma(r.cmb.heliocentric.uncertainty);
        assert!((z - 85.0 / SPEED_OF_LIGHT).abs() < 1e-12, "{z}");
    }
}

#[test]
fn a_scatter_per_one_plus_z_grows_with_each_redshift() {
    // WISE × SuperCOSMOS states σz / (1 + z) = 0.033 (Bilicki et al. 2016).
    let d = with_catalogue_error(
        "sdss-dr17-galaxies",
        CatalogueError {
            value: 0.033,
            unit: Unit::Dimensionless,
            times_one_plus_z: true,
            basis: "Bilicki et al. 2016: σz/(1+z) = 0.033".to_owned(),
        },
    );
    let n = normalize_body(&d, &fs::read(fixture_of(&d)).unwrap());
    assert_ne!(n.entries.len(), 0);
    for e in &n.entries {
        let z = e.redshifts[0].cmb.heliocentric;
        let expected = 0.033 * (1.0 + z.value);
        assert!((sigma(z.uncertainty) - expected).abs() < 1e-15);
    }
}

/// The repository's `sixdfgs-dr3` definition with its `error: e_cz` line replaced by `with`.
fn refused_field(with: &str) -> String {
    let text = fs::read_to_string("../../sources/sixdfgs-dr3.yaml").unwrap();
    assert!(text.contains("      error: e_cz\n"));
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("sixdfgs-dr3.yaml"),
        text.replace("      error: e_cz\n", with),
    )
    .unwrap();
    read_all(dir.path()).unwrap_err().field
}

#[test]
fn a_catalogue_error_beside_an_error_column_or_scaled_on_a_velocity_is_refused() {
    let beside = "      error: e_cz\n      catalogue_error: { value: 85, unit: kilometre_per_second, basis: \"b\" }\n";
    assert_eq!(
        refused_field(beside),
        "normalization.redshifts[0].catalogue_error"
    );
    let scaled = "      catalogue_error: { value: 0.03, unit: kilometre_per_second, times_one_plus_z: true, basis: \"b\" }\n";
    assert_eq!(
        refused_field(scaled),
        "normalization.redshifts[0].catalogue_error"
    );
    let unbased =
        "      catalogue_error: { value: 85, unit: kilometre_per_second, basis: \" \" }\n";
    assert_eq!(
        refused_field(unbased),
        "normalization.redshifts[0].catalogue_error.basis"
    );
}
