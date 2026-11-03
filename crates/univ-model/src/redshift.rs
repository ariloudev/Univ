//! Redshifts as measured and in the frame of the cosmic microwave background.

use serde::{Deserialize, Serialize};

use crate::cosmology::SPEED_OF_LIGHT;
use crate::position::{ICRS_TO_GALACTIC, Vec3, dot, unit_vector};
use crate::value::Quantity;

/// The frame a redshift is published in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedshiftFrame {
    Heliocentric,
    /// Corrected to the barycentre of the Local Group.
    LocalGroup,
    /// Corrected to the rest frame of the cosmic microwave background.
    Cmb,
}

/// The Sun's motion against the cosmic microwave background.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dipole {
    /// Speed in km/s.
    pub speed: f64,
    /// The apex in galactic longitude and latitude, degrees.
    pub l: f64,
    pub b: f64,
    pub citation: String,
}

impl Dipole {
    /// Planck 2018 (Planck Collaboration 2020, A&A 641, A1): 369.82 ± 0.11 km/s toward
    /// l = 264.021°, b = 48.253°.
    #[must_use]
    pub fn planck_2018() -> Self {
        Self {
            speed: 369.82,
            l: 264.021,
            b: 48.253,
            citation: "Planck Collaboration 2020, A&A 641, A1".to_owned(),
        }
    }

    /// The unit vector toward the apex in ICRS.
    #[must_use]
    pub fn apex_icrs(&self) -> Vec3 {
        ICRS_TO_GALACTIC
            .transpose()
            .apply(unit_vector(self.l, self.b))
    }
}

/// A redshift in the CMB frame, with the measured redshift and dipole it was derived from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CmbRedshift {
    /// The dimensionless CMB-frame redshift.
    pub value: Quantity,
    /// The heliocentric redshift it was derived from, as measured.
    pub heliocentric: Quantity,
    pub dipole: Dipole,
}

/// The CMB-frame redshift of a source in ICRS direction `direction` (a unit vector) with the
/// heliocentric redshift `heliocentric`: `1 + z_cmb = (1 + z_hel) · γ (1 + β cos θ)`, where θ is
/// the angle between the source and the apex. The relativistic Doppler factor composes with the
/// cosmological redshift; to first order `cz` gains `(1 + z_hel) v cos θ`.
#[must_use]
pub fn to_cmb_frame(heliocentric: Quantity, direction: Vec3, dipole: &Dipole) -> CmbRedshift {
    let beta = dipole.speed / SPEED_OF_LIGHT;
    let gamma = 1.0 / (1.0 - beta * beta).sqrt();
    let factor = gamma * (1.0 + beta * dot(direction, dipole.apex_icrs()));
    let value = Quantity {
        value: (1.0 + heliocentric.value) * factor - 1.0,
        unit: heliocentric.unit,
        uncertainty: heliocentric.uncertainty.scaled(factor),
    };
    CmbRedshift {
        value,
        heliocentric,
        dipole: dipole.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::lon_lat;
    use crate::units::Unit;
    use crate::value::Uncertainty;

    fn z(value: f64) -> Quantity {
        Quantity::new(
            value,
            Unit::Dimensionless,
            Uncertainty::Symmetric { sigma: 1e-4 },
        )
        .unwrap()
    }

    fn change_in_cz(direction: Vec3) -> f64 {
        let dipole = Dipole::planck_2018();
        let cmb = to_cmb_frame(z(0.01), direction, &dipole);
        (cmb.value.value - cmb.heliocentric.value) * SPEED_OF_LIGHT
    }

    #[test]
    fn the_planck_2018_dipole_is_used_and_its_apex_matches_astropy_in_icrs() {
        let dipole = Dipole::planck_2018();
        assert_eq!(
            (dipole.speed, dipole.l, dipole.b),
            (369.82, 264.021, 48.253)
        );
        let (ra, dec) = lon_lat(dipole.apex_icrs());
        assert!(
            (ra - 167.941_903).abs() < 1e-4 && (dec + 6.944_260).abs() < 1e-4,
            "{ra} {dec}"
        );
    }

    #[test]
    fn cz_rises_toward_the_apex_falls_away_from_it_and_holds_across_it() {
        let apex = Dipole::planck_2018().apex_icrs();
        let toward = change_in_cz(apex);
        assert!(
            (toward - 1.01 * 369.82).abs() < 0.5,
            "toward the apex: {toward}"
        );
        let away = change_in_cz([-apex[0], -apex[1], -apex[2]]);
        assert!(
            (away + 1.01 * 369.82).abs() < 0.5,
            "away from the apex: {away}"
        );
        // At 90° only the transverse Doppler term, (1 + z) (γ - 1) c ≈ 0.23 km/s, remains.
        let across = change_in_cz(
            crate::position::cross(apex, [0.0, 0.0, 1.0])
                .map(|c| c / (1.0 - apex[2] * apex[2]).sqrt()),
        );
        assert!(across.abs() < 0.5, "across the apex: {across}");
    }

    #[test]
    fn the_derivation_keeps_the_measured_redshift_and_the_dipole() {
        let measured = z(0.023);
        let cmb = to_cmb_frame(measured, [1.0, 0.0, 0.0], &Dipole::planck_2018());
        assert_eq!(cmb.heliocentric, measured);
        assert_eq!(cmb.dipole, Dipole::planck_2018());
        let none = Quantity::new(0.02, Unit::Dimensionless, Uncertainty::NoneReported).unwrap();
        let cmb = to_cmb_frame(none, [1.0, 0.0, 0.0], &Dipole::planck_2018());
        assert_eq!(cmb.value.uncertainty, Uncertainty::NoneReported);
    }
}
