//! A row's redshift readings, each moved to the CMB frame or read as published there.

use univ_model::cosmology::{Cosmology, SPEED_OF_LIGHT};
use univ_model::entry::{Redshift, RedshiftKind};
use univ_model::position::Vec3;
use univ_model::redshift::RedshiftFrame;
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity};

use super::super::plan::RedshiftPlan;
use super::{Row, entry_reason};
use crate::source::LeftOutReason;

impl Row<'_, '_> {
    /// The redshift reading `r` gives this row, seen in `direction`: none when its cell is
    /// blank. A row whose value lies below the reading's floor is left out.
    pub(super) fn redshift(
        &mut self,
        r: &RedshiftPlan,
        direction: Option<Vec3>,
    ) -> Result<Option<Redshift>, LeftOutReason> {
        let Some(mut measured) = self.quantity(&r.measured)? else {
            return Ok(None);
        };
        if r.floor.is_some_and(|below| measured.value < below) {
            return Err(LeftOutReason::UnsuitableRedshift);
        }
        let mut read_as = None;
        if let Some((from, cosmology, kind_of_value)) = &r.comoving {
            let comoving;
            (measured, comoving) = comoving_redshift(measured, *from, cosmology)?;
            read_as = comoving.then_some(*kind_of_value);
        }
        let kind = self.by_code(&r.kind)?;
        let default = read_as.unwrap_or(match kind {
            RedshiftKind::Spectroscopic => KindOfValue::Measured,
            RedshiftKind::Photometric => KindOfValue::Estimated,
        });
        let kind_of_value = self.kind(r.measured.value.index, default);
        let direction = direction.ok_or(LeftOutReason::MissingPosition)?;
        let correction = self.cx.correction;
        match r.frame {
            RedshiftFrame::Cmb => {
                Redshift::published_cmb(measured, kind, kind_of_value, direction, correction)
            }
            frame => {
                Redshift::measured(frame, measured, kind, kind_of_value, direction, correction)
            }
        }
        .map(Some)
        .map_err(entry_reason)
    }
}

/// `measured`, a velocity standing for H0 times a comoving distance in `cosmology` from `from`
/// km/s up, as the redshift at that distance, its uncertainty scaled by dz/dv = E(z) / c, and
/// true; below `from`, it is as published, a cz, and false.
fn comoving_redshift(
    measured: Quantity,
    from: f64,
    cosmology: &Cosmology,
) -> Result<(Quantity, bool), LeftOutReason> {
    let v = measured
        .to(Unit::KilometrePerSecond)
        .map_err(|_| LeftOutReason::UnusableValue)?;
    if v.value < from {
        return Ok((measured, false));
    }
    let z = cosmology
        .redshift_at(v.value / cosmology.hubble_constant)
        .map_err(|_| LeftOutReason::UnusableValue)?;
    let om = cosmology.omega_matter;
    let expansion = (om * (1.0 + z).powi(3) + 1.0 - om).sqrt();
    Quantity::new(
        z,
        Unit::Dimensionless,
        v.uncertainty.scaled(expansion / SPEED_OF_LIGHT),
    )
    .map(|q| (q, true))
    .map_err(|_| LeftOutReason::UnusableValue)
}
