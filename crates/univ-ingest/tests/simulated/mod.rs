//! Simulated catalogue entries for the matching and ranking tests. Every entry built here is
//! made up: positions, velocities and distances chosen to sit inside or outside the tolerances
//! of `rules/matching.yaml`, under the ids of real sources only so that the real rules apply.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::panic,
    reason = "helpers shared by two test crates, each using a subset; a failed unwrap or panic is the test failing"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use univ_ingest::matching::MatchingRules;
use univ_ingest::ranking::RankingRules;
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{
    CatalogueEntry, NativePosition, Position, PositionErrors, PositionUncertainty, Redshift,
    RedshiftKind,
};
use univ_model::ids::Designation;
use univ_model::position::{Epoch, Frame, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

pub use super::common::source_id as source;

pub mod tiles;

pub fn rules_dir() -> PathBuf {
    super::common::workspace().join("rules")
}

/// The repository's matching rules.
pub fn matching_rules() -> MatchingRules {
    MatchingRules::read(&rules_dir().join("matching.yaml")).unwrap_or_else(|e| panic!("{e}"))
}

/// The repository's ranking rule.
pub fn ranking_rules() -> RankingRules {
    RankingRules::read(&rules_dir().join("ranking.yaml")).unwrap_or_else(|e| panic!("{e}"))
}

pub fn designation(src: &str, id: &str) -> Designation {
    Designation::new(source(src), id).unwrap()
}

/// The position `east` and `north` arcseconds from (`ra`, `dec`), on the tangent plane.
pub fn offset(ra: f64, dec: f64, east: f64, north: f64) -> (f64, f64) {
    let ra = ra + east / 3_600.0 / dec.to_radians().cos();
    (ra.rem_euclid(360.0), dec + north / 3_600.0)
}

pub fn value(v: f64, unit: Unit, to: Unit, u: Uncertainty) -> Value {
    Value::normalize(
        Quantity::new(v, unit, u).unwrap(),
        to,
        KindOfValue::Measured,
    )
    .unwrap()
}

/// An entry of `src` named `id` at an ICRS J2000 position, with nothing else.
pub fn entry(src: &str, id: &str, ra: f64, dec: f64) -> CatalogueEntry {
    let native = NativePosition {
        frame: Frame::Icrs,
        lon: ra,
        lat: dec,
        unit: Unit::Degree,
        epoch: Epoch::J2000,
    };
    CatalogueEntry {
        position: Some(Position::new(native, PositionUncertainty::NoneReported).unwrap()),
        ..bare(src, id)
    }
}

/// An entry of `src` named `id` without a position, such as a Bailer-Jones distance.
pub fn bare(src: &str, id: &str) -> CatalogueEntry {
    CatalogueEntry {
        designation: designation(src, id),
        name: None,
        position: None,
        distances: Vec::new(),
        redshifts: Vec::new(),
        velocity: None,
        parallax: None,
        proper_motion: None,
        magnitude: None,
        colours: Vec::new(),
        spectral_type: None,
        group_id: None,
        supergalactic_velocity: None,
        flags: BTreeMap::new(),
    }
}

/// The entry with a published position error of `mas` in each coordinate, at `epoch`.
pub fn with_error(mut e: CatalogueEntry, mas: f64, epoch: f64) -> CatalogueEntry {
    let p = e.position.take().unwrap();
    let native = NativePosition {
        epoch: Epoch(epoch),
        ..p.native
    };
    let error = || {
        value(
            mas,
            Unit::Milliarcsecond,
            Unit::Degree,
            Uncertainty::NoneReported,
        )
    };
    let errors = PositionErrors {
        ra: error(),
        dec: error(),
    };
    let u = PositionUncertainty::Published(Box::new(errors));
    e.position = Some(Position::new(native, u).unwrap());
    e
}

/// The entry with a heliocentric spectroscopic cz of `cz` ± `sigma` km/s.
pub fn with_cz(mut e: CatalogueEntry, cz: f64, sigma: f64) -> CatalogueEntry {
    let p = e.position.as_ref().unwrap().icrs;
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let q = Quantity::new(
        cz,
        Unit::KilometrePerSecond,
        Uncertainty::Symmetric { sigma },
    )
    .unwrap();
    let r = Redshift::heliocentric(
        q,
        RedshiftKind::Spectroscopic,
        KindOfValue::Measured,
        unit_vector(p.ra, p.dec),
        &correction,
    )
    .unwrap();
    e.redshifts.push(r);
    e
}

/// The entry with a distance modulus `mu` ± `sigma` measured by `how`, in Mpc.
pub fn with_modulus(
    mut e: CatalogueEntry,
    mu: f64,
    sigma: f64,
    how: (DistanceKind, Method),
) -> CatalogueEntry {
    let q = Quantity::new(mu, Unit::Magnitude, Uncertainty::Symmetric { sigma }).unwrap();
    let d = Distance::new(
        q,
        None,
        Unit::Megaparsec,
        KindOfValue::Measured,
        how,
        Measure::Luminosity,
        None,
    )
    .unwrap();
    e.distances.push(d);
    e
}

/// The entry with a distance of `mpc` ± `sigma` Mpc (none reported when `sigma` is none).
pub fn with_distance(
    mut e: CatalogueEntry,
    mpc: f64,
    sigma: Option<f64>,
    how: (DistanceKind, Method),
) -> CatalogueEntry {
    let u = sigma.map_or(Uncertainty::NoneReported, |sigma| Uncertainty::Symmetric {
        sigma,
    });
    let q = Quantity::new(mpc, Unit::Megaparsec, u).unwrap();
    let d = Distance::new(
        q,
        None,
        Unit::Megaparsec,
        KindOfValue::Measured,
        how,
        Measure::Unspecified,
        None,
    )
    .unwrap();
    e.distances.push(d);
    e
}

/// The entry with the name `name` beside its identifier.
pub fn named(mut e: CatalogueEntry, name: &str) -> CatalogueEntry {
    e.name = Some(name.to_owned());
    e
}

/// The entry with a parallax of `mas` ± `sigma` mas, as published, normalized to degrees.
pub fn with_parallax(mut e: CatalogueEntry, mas: f64, sigma: f64) -> CatalogueEntry {
    e.parallax = Some(value(
        mas,
        Unit::Milliarcsecond,
        Unit::Degree,
        Uncertainty::Symmetric { sigma },
    ));
    e
}

/// The entry with a heliocentric spectroscopic cz of `cz` km/s, published without an error.
pub fn with_bare_cz(mut e: CatalogueEntry, cz: f64) -> CatalogueEntry {
    let p = e.position.as_ref().unwrap().icrs;
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let q = Quantity::new(cz, Unit::KilometrePerSecond, Uncertainty::NoneReported).unwrap();
    let r = Redshift::heliocentric(
        q,
        RedshiftKind::Spectroscopic,
        KindOfValue::Measured,
        unit_vector(p.ra, p.dec),
        &correction,
    )
    .unwrap();
    e.redshifts.push(r);
    e
}
