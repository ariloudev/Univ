use std::collections::BTreeMap;

use super::*;
use crate::distance::{DistanceKind, Measure, Method};
use crate::ids::SourceId;
use crate::position::Epoch;
use crate::redshift::DipoleModel;
use crate::value::Uncertainty;

fn value(v: f64, unit: Unit, to: Unit) -> Value {
    let native = Quantity::new(v, unit, Uncertainty::NoneReported).unwrap();
    Value::normalize(native, to, KindOfValue::Measured).unwrap()
}

fn entry() -> CatalogueEntry {
    let native = NativePosition {
        frame: Frame::Fk5J2000,
        lon: 10.684_71,
        lat: 41.268_75,
        unit: Unit::Degree,
        epoch: Epoch::J2000,
    };
    let position = Position::new(native, PositionUncertainty::NoneReported).unwrap();
    let cz = Quantity::new(
        -300.0,
        Unit::KilometrePerSecond,
        Uncertainty::Symmetric { sigma: 4.0 },
    )
    .unwrap();
    let redshift = Redshift::heliocentric(
        cz,
        RedshiftKind::Spectroscopic,
        KindOfValue::Measured,
        position.icrs.unit_vector(),
        &CmbCorrection::new(DipoleModel::Planck2018),
    )
    .unwrap();
    let mu = Quantity::new(24.47, Unit::Magnitude, Uncertainty::NoneReported).unwrap();
    let how = (DistanceKind::PrimaryIndicator, Method::Cepheids);
    let distance = Distance::new(
        mu,
        None,
        Unit::Megaparsec,
        KindOfValue::Measured,
        how,
        Measure::Luminosity,
        None,
    )
    .unwrap();
    CatalogueEntry {
        cross_ids: Flags::default(),
        designation: Designation::new(SourceId::new("twomrs").unwrap(), "00424433+4116074")
            .unwrap(),
        name: None,
        position: Some(position),
        distances: thin_vec::thin_vec![distance],
        redshifts: thin_vec::thin_vec![redshift],
        velocity: Some(Box::new(value(
            -300.0,
            Unit::KilometrePerSecond,
            Unit::KilometrePerSecond,
        ))),
        parallax: None,
        proper_motion: None,
        magnitude: Some(Box::new(Magnitude {
            band: "Kcmag".to_owned(),
            value: value(0.797, Unit::Magnitude, Unit::Magnitude),
        })),
        colours: thin_vec::ThinVec::new(),
        ages: thin_vec::ThinVec::new(),
        spectral_type: None,
        group_id: None,
        supergalactic_velocity: None,
        flags: BTreeMap::from([("r_cz".to_owned(), "1991RC3.9.C...0000d".to_owned())]).into(),
        references: Box::default(),
    }
}

#[test]
fn a_redshift_published_as_cz_keeps_cz_and_derives_z_and_its_cmb_value() {
    let e = entry();
    let r = &e.redshifts[0];
    assert_eq!(r.velocity.as_deref().unwrap().value, -300.0);
    let z = r.cmb.measured;
    assert!((z.value + 300.0 / SPEED_OF_LIGHT).abs() < 1e-15);
    assert_eq!(
        z.uncertainty,
        Uncertainty::Symmetric {
            sigma: 4.0 / SPEED_OF_LIGHT
        }
    );
    assert_eq!(r.cmb.dipole, DipoleModel::Planck2018);
}

#[test]
fn an_entry_reads_back_as_it_was_written_and_is_checked_when_read() {
    let e = entry();
    let yaml = serde_norway::to_string(&e).unwrap();
    assert_eq!(serde_norway::from_str::<CatalogueEntry>(&yaml).unwrap(), e);

    let mut wrong = entry();
    wrong.velocity = Some(Box::new(value(
        300.0,
        Unit::KilometrePerSecond,
        Unit::MetrePerSecond,
    )));
    let yaml = serde_norway::to_string(&wrong).unwrap();
    assert!(serde_norway::from_str::<CatalogueEntry>(&yaml).is_err());

    let mut moved = entry();
    if let Some(p) = &mut moved.position {
        p.icrs.dec = 41.0;
    }
    let yaml = serde_norway::to_string(&moved).unwrap();
    assert!(serde_norway::from_str::<CatalogueEntry>(&yaml).is_err());
}

#[test]
fn angles_are_normalized_to_degrees_and_distances_to_one_unit() {
    let mut e = entry();
    e.parallax = Some(Box::new(value(
        10.0,
        Unit::Milliarcsecond,
        Unit::Milliarcsecond,
    )));
    assert_eq!(
        e.check(),
        Err(EntryError::Unit("parallax", Unit::Milliarcsecond))
    );
    e.parallax = Some(Box::new(value(10.0, Unit::Milliarcsecond, Unit::Degree)));
    assert_eq!(e.check(), Ok(()));
    let mut parsecs = e.distances[0].clone();
    parsecs.normalized = parsecs.normalized.to(Unit::Parsec).unwrap();
    e.distances.push(parsecs);
    assert_eq!(e.check(), Err(EntryError::MixedDistanceUnits));
}

#[test]
fn a_colour_index_is_in_magnitudes_and_a_spectral_type_is_never_blank() {
    let mut e = entry();
    let colour = |unit| ColourIndex {
        bands: ["B".to_owned(), "V".to_owned()],
        value: value(0.65, unit, unit),
        derived_from: None,
    };
    e.colours = thin_vec::thin_vec![colour(Unit::Magnitude)];
    e.spectral_type = Some(Box::new(SpectralType {
        text: "G2V".to_owned(),
        kind: KindOfValue::Measured,
    }));
    assert_eq!(e.check(), Ok(()));
    e.colours = thin_vec::thin_vec![colour(Unit::Dimensionless)];
    assert_eq!(
        e.check(),
        Err(EntryError::Unit("colour index", Unit::Dimensionless))
    );
    e.colours.clear();
    e.spectral_type = Some(Box::new(SpectralType {
        text: " ".to_owned(),
        kind: KindOfValue::Measured,
    }));
    assert_eq!(e.check(), Err(EntryError::Blank("spectral type")));
}

#[test]
fn an_age_reads_back_and_an_entry_without_ages_still_reads() {
    let interval = Uncertainty::CredibleInterval {
        minus: 0.3,
        plus: 0.2,
        probability: 0.68,
    };
    let native = Quantity::new(7.3, Unit::LogYear, interval).unwrap();
    let mut aged = entry();
    aged.ages = thin_vec::thin_vec![Age::normalize(native, KindOfValue::Estimated).unwrap()];
    let yaml = serde_norway::to_string(&aged).unwrap();
    assert_eq!(
        serde_norway::from_str::<CatalogueEntry>(&yaml).unwrap(),
        aged
    );

    // An entry normalized before ages were read has no `ages` field.
    let old = serde_norway::to_string(&entry()).unwrap();
    assert!(old.contains("ages: []\n"));
    let old = old.replace("ages: []\n", "");
    assert!(!old.contains("ages:"));
    assert_eq!(
        serde_norway::from_str::<CatalogueEntry>(&old)
            .unwrap()
            .ages
            .len(),
        0
    );
}

#[test]
fn an_age_normalized_to_anything_but_years_is_refused() {
    let mut wrong = entry();
    wrong.ages = thin_vec::thin_vec![Age {
        value: value(2.5e7, Unit::Parsec, Unit::Parsec),
    }];
    assert_eq!(wrong.check(), Err(EntryError::Unit("age", Unit::Parsec)));
}
