//! Redshifts as measured and in the frame of the cosmic microwave background.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cosmology::SPEED_OF_LIGHT;
use crate::position::{Frame, Vec3, between, dot, unit_vector};
use crate::units::Unit;
use crate::value::{Quantity, Uncertainty, ValueError};

/// The frame a redshift is published in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedshiftFrame {
    #[default]
    Heliocentric,
    /// Corrected to the barycentre of the Local Group.
    LocalGroup,
    /// Corrected to the rest frame of the cosmic microwave background.
    Cmb,
}

/// A measurement of the Sun's motion against the cosmic microwave background, named in each
/// redshift derived with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DipoleModel {
    /// Planck Collaboration 2020 (A&A 641, A1): 369.82 ± 0.11 km/s toward l = 264.021°,
    /// b = 48.253°.
    Planck2018,
}

/// The parameters of a dipole model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dipole {
    /// Speed and its standard error, in km/s.
    pub speed: f64,
    pub speed_sigma: f64,
    /// The apex in galactic longitude and latitude, degrees.
    pub l: f64,
    pub b: f64,
    pub citation: &'static str,
}

impl DipoleModel {
    #[must_use]
    pub fn dipole(self) -> Dipole {
        match self {
            Self::Planck2018 => Dipole {
                speed: 369.82,
                speed_sigma: 0.11,
                l: 264.021,
                b: 48.253,
                citation: "Planck Collaboration 2020, A&A 641, A1",
            },
        }
    }
}

/// A redshift in the CMB frame, with the measured redshift and dipole model it was derived from.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CmbRedshift {
    /// The dimensionless CMB-frame redshift.
    pub value: Quantity,
    /// The heliocentric redshift it was derived from, as measured; for a redshift published in
    /// the CMB frame, the heliocentric redshift derived back from it with `dipole`, only to
    /// compare it with heliocentric ones.
    pub heliocentric: Quantity,
    pub dipole: DipoleModel,
    /// The frame its source published it in: heliocentric, `value` then derived with `dipole`;
    /// or the CMB frame, `value` then the published redshift itself, with no dipole applied.
    #[serde(default, skip_serializing_if = "is_heliocentric")]
    pub published: RedshiftFrame,
}

impl CmbRedshift {
    /// The redshift in the frame its source published it in: the heliocentric one, or the
    /// CMB-frame one.
    #[must_use]
    pub fn as_published(&self) -> Quantity {
        match self.published {
            RedshiftFrame::Cmb => self.value,
            _ => self.heliocentric,
        }
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_heliocentric(frame: &RedshiftFrame) -> bool {
    *frame == RedshiftFrame::Heliocentric
}

/// A redshift that cannot be moved to the CMB frame.
#[derive(Debug, Error, PartialEq)]
pub enum RedshiftError {
    #[error("only a heliocentric redshift is corrected here, not one in the {0:?} frame")]
    NotHeliocentric(RedshiftFrame),
    #[error("a redshift is dimensionless, not in {0:?}")]
    NotDimensionless(Unit),
    #[error("the direction is not a unit vector")]
    NotAUnitVector,
    #[error(transparent)]
    Value(#[from] ValueError),
}

/// The correction of one dipole model, worked out once for many redshifts.
#[derive(Clone, Copy, Debug)]
pub struct CmbCorrection {
    model: DipoleModel,
    apex: Vec3,
    beta: f64,
    beta_sigma: f64,
    gamma: f64,
}

impl CmbCorrection {
    #[must_use]
    pub fn new(model: DipoleModel) -> Self {
        let d = model.dipole();
        let beta = d.speed / SPEED_OF_LIGHT;
        Self {
            model,
            apex: between(Frame::Galactic, Frame::Icrs).apply(unit_vector(d.l, d.b)),
            beta,
            beta_sigma: d.speed_sigma / SPEED_OF_LIGHT,
            gamma: 1.0 / (1.0 - beta * beta).sqrt(),
        }
    }

    /// The unit vector toward the apex in ICRS.
    #[must_use]
    pub fn apex_icrs(&self) -> Vec3 {
        self.apex
    }

    /// The heliocentric redshift that this correction moves to the CMB-frame redshift `cmb` in
    /// the ICRS unit `direction`: the inverse of [`Self::apply`], `1 + z_hel = (1 + z_cmb) /
    /// (γ (1 + β cos θ))`. For a source that publishes only a CMB-frame value, to compare it with
    /// heliocentric ones; it is not kept as a measurement.
    #[must_use]
    pub fn heliocentric_of(&self, cmb: f64, direction: Vec3) -> f64 {
        let factor = self.gamma * (1.0 + self.beta * dot(direction, self.apex));
        (1.0 + cmb) / factor - 1.0
    }

    /// The CMB-frame redshift of a source in the ICRS unit `direction` with the heliocentric
    /// redshift `measured`: `1 + z_cmb = (1 + z_hel) γ (1 + β cos θ)`, θ the angle between the
    /// source and the apex. To first order `cz` gains `(1 + z_hel) v cos θ`. A symmetric error
    /// gains the dipole's speed error in quadrature; other forms of uncertainty are scaled.
    ///
    /// # Errors
    ///
    /// Fails on a redshift not heliocentric or not dimensionless, or a direction that is not a
    /// unit vector.
    pub fn apply(
        &self,
        measured: Quantity,
        frame: RedshiftFrame,
        direction: Vec3,
    ) -> Result<CmbRedshift, RedshiftError> {
        if frame != RedshiftFrame::Heliocentric {
            return Err(RedshiftError::NotHeliocentric(frame));
        }
        if measured.unit != Unit::Dimensionless {
            return Err(RedshiftError::NotDimensionless(measured.unit));
        }
        if !direction.iter().all(|c| c.is_finite())
            || (dot(direction, direction) - 1.0).abs() >= 1e-9
        {
            return Err(RedshiftError::NotAUnitVector);
        }
        let cos = dot(direction, self.apex);
        let factor = self.gamma * (1.0 + self.beta * cos);
        let uncertainty = match measured.uncertainty {
            Uncertainty::Symmetric { sigma } => Uncertainty::Symmetric {
                sigma: (factor * sigma).hypot((1.0 + measured.value) * cos * self.beta_sigma),
            },
            other => other.scaled(factor),
        };
        let value = Quantity::new(
            (1.0 + measured.value) * factor - 1.0,
            measured.unit,
            uncertainty,
        )?;
        Ok(CmbRedshift {
            value,
            heliocentric: measured,
            dipole: self.model,
            published: RedshiftFrame::Heliocentric,
        })
    }

    /// A redshift its source published in the CMB frame, kept as published: the dipole step is
    /// not applied to it, which its source has already made. Its heliocentric redshift is
    /// derived back with [`Self::heliocentric_of`], only to compare it with heliocentric ones,
    /// and keeps the published redshift's uncertainty.
    ///
    /// # Errors
    ///
    /// Fails on a redshift not dimensionless, or a direction that is not a unit vector.
    pub fn published_cmb(
        &self,
        published: Quantity,
        direction: Vec3,
    ) -> Result<CmbRedshift, RedshiftError> {
        if published.unit != Unit::Dimensionless {
            return Err(RedshiftError::NotDimensionless(published.unit));
        }
        if !direction.iter().all(|c| c.is_finite())
            || (dot(direction, direction) - 1.0).abs() >= 1e-9
        {
            return Err(RedshiftError::NotAUnitVector);
        }
        let heliocentric = Quantity::new(
            self.heliocentric_of(published.value, direction),
            Unit::Dimensionless,
            published.uncertainty,
        )?;
        Ok(CmbRedshift {
            value: published,
            heliocentric,
            dipole: self.model,
            published: RedshiftFrame::Cmb,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{cross, lon_lat};

    fn z(value: f64) -> Quantity {
        Quantity::new(
            value,
            Unit::Dimensionless,
            Uncertainty::Symmetric { sigma: 1e-4 },
        )
        .unwrap()
    }

    fn planck() -> CmbCorrection {
        CmbCorrection::new(DipoleModel::Planck2018)
    }

    fn change_in_cz(direction: Vec3) -> f64 {
        let cmb = planck()
            .apply(z(0.01), RedshiftFrame::Heliocentric, direction)
            .unwrap();
        (cmb.value.value - cmb.heliocentric.value) * SPEED_OF_LIGHT
    }

    #[test]
    fn heliocentric_of_undoes_the_correction() {
        let direction = unit_vector(187.7, 12.4);
        let cmb = planck()
            .apply(z(0.005), RedshiftFrame::Heliocentric, direction)
            .unwrap();
        let back = planck().heliocentric_of(cmb.value.value, direction);
        assert!((back - 0.005).abs() < 1e-12, "{back}");
        assert!((cmb.value.value - 0.005).abs() * SPEED_OF_LIGHT > 100.0);
    }

    #[test]
    fn a_redshift_published_in_the_cmb_frame_is_kept_with_no_dipole_applied() {
        let direction = unit_vector(187.7, 12.4);
        let cmb = planck().published_cmb(z(0.005), direction).unwrap();
        assert_eq!(cmb.value, z(0.005), "as published");
        assert_eq!(cmb.published, RedshiftFrame::Cmb);
        // Its heliocentric redshift is derived back, and the correction takes it to the value.
        let again = planck()
            .apply(cmb.heliocentric, RedshiftFrame::Heliocentric, direction)
            .unwrap();
        assert!((again.value.value - 0.005).abs() < 1e-12);
        assert!((cmb.heliocentric.value - 0.005).abs() * SPEED_OF_LIGHT > 100.0);
        let json = serde_norway::to_string(&cmb).unwrap();
        assert!(json.contains("published: cmb"), "{json}");
        let read: CmbRedshift = serde_norway::from_str(&json).unwrap();
        assert_eq!(read, cmb);
        // A heliocentric one does not write the field, and reads back heliocentric.
        let measured = planck()
            .apply(z(0.005), RedshiftFrame::Heliocentric, direction)
            .unwrap();
        let json = serde_norway::to_string(&measured).unwrap();
        assert!(!json.contains("published"), "{json}");
        let read: CmbRedshift = serde_norway::from_str(&json).unwrap();
        assert_eq!(read.published, RedshiftFrame::Heliocentric);
    }

    #[test]
    fn the_planck_2018_dipole_is_used_and_its_apex_matches_astropy_in_icrs() {
        let dipole = DipoleModel::Planck2018.dipole();
        assert_eq!(
            (dipole.speed, dipole.l, dipole.b),
            (369.82, 264.021, 48.253)
        );
        let (ra, dec) = lon_lat(planck().apex_icrs());
        assert!(
            (ra - 167.941_903).abs() < 1e-4 && (dec + 6.944_260).abs() < 1e-4,
            "{ra} {dec}"
        );
    }

    #[test]
    fn cz_rises_toward_the_apex_falls_away_from_it_and_holds_across_it() {
        let apex = planck().apex_icrs();
        let toward = change_in_cz(apex);
        assert!(
            (toward - 1.01 * 369.82).abs() < 0.5,
            "toward the apex: {toward}"
        );
        let away = change_in_cz(apex.map(|c| -c));
        assert!(
            (away + 1.01 * 369.82).abs() < 0.5,
            "away from the apex: {away}"
        );
        // At 90° only the transverse Doppler term, (1 + z) (γ - 1) c ≈ 0.23 km/s, remains.
        let side = cross(apex, [0.0, 0.0, 1.0]);
        let across = change_in_cz(side.map(|c| c / dot(side, side).sqrt()));
        assert!(across.abs() < 0.5, "across the apex: {across}");
    }

    #[test]
    fn the_derivation_keeps_the_measured_redshift_and_the_dipole() {
        let measured = z(0.023);
        let cmb = planck()
            .apply(measured, RedshiftFrame::Heliocentric, [1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(cmb.heliocentric, measured);
        assert_eq!(cmb.dipole, DipoleModel::Planck2018);
        let none = Quantity::new(0.02, Unit::Dimensionless, Uncertainty::NoneReported).unwrap();
        let cmb = planck()
            .apply(none, RedshiftFrame::Heliocentric, [1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(cmb.value.uncertainty, Uncertainty::NoneReported);
    }

    #[test]
    fn redshifts_already_corrected_or_not_dimensionless_are_refused() {
        let x = [1.0, 0.0, 0.0];
        let refused = planck().apply(z(0.02), RedshiftFrame::LocalGroup, x);
        assert_eq!(
            refused,
            Err(RedshiftError::NotHeliocentric(RedshiftFrame::LocalGroup))
        );
        let cz = Quantity::new(6_000.0, Unit::KilometrePerSecond, Uncertainty::NoneReported);
        let refused = planck().apply(cz.unwrap(), RedshiftFrame::Heliocentric, x);
        assert_eq!(
            refused,
            Err(RedshiftError::NotDimensionless(Unit::KilometrePerSecond))
        );
        let refused = planck().apply(z(0.02), RedshiftFrame::Heliocentric, [2.0, 0.0, 0.0]);
        assert_eq!(refused, Err(RedshiftError::NotAUnitVector));
    }
}
