//! An object's own row: its preferred sky position, its preferred measurement with the ranking
//! that chose it, and its derived position, whose chain is its template expanded.

use super::columns::Columns;
use super::{ReadError, Vocabulary};
use crate::api::objects::{
    DerivedPositionRecord, EntryRecord, MeasuredQuantity, MeasurementKey, MeasurementRecord,
    PreferredMeasurement, RunnerUp, SkyPositionRecord,
};
use crate::derive::Step;
use crate::object::FractionalUncertainty;
use crate::position::{BARYCENTRE, Frame, ICRS_CARTESIAN_NAME};
use crate::release::chains::expand;
use crate::units::Unit;
use crate::value::Quantity;

/// The object's preferred sky position, none when it has none.
pub(super) fn sky(c: &Columns<'_>) -> Result<Option<SkyPositionRecord>, ReadError> {
    let Some(ra) = c.real_opt("ra")? else {
        return Ok(None);
    };
    Ok(Some(SkyPositionRecord {
        frame: Frame::Icrs,
        ra,
        dec: c.real("dec")?,
        epoch: c.real("epoch")?,
        entry: c.small("position_entry")?,
        class: c.typed("position_class")?,
        uncertainty_mas: c.real_opt("position_uncertainty_mas")?,
    }))
}

/// The measurement of columns `{prefix}entry`, `quantity` and `n`, none when its quantity is
/// NULL.
fn key(c: &Columns<'_>, prefix: &str) -> Result<Option<MeasurementKey>, ReadError> {
    let Some(quantity) = c.typed_opt(&format!("{prefix}quantity"))? else {
        return Ok(None);
    };
    Ok(Some(MeasurementKey {
        entry: c.small(&format!("{prefix}entry"))?,
        quantity,
        n: c.small(&format!("{prefix}n"))?,
    }))
}

/// The fractional uncertainty of columns `{prefix}total`, `measurement` and `velocity`.
fn fraction(c: &Columns<'_>, prefix: &str) -> Result<FractionalUncertainty, ReadError> {
    Ok(match c.real_opt(&format!("{prefix}total"))? {
        None => FractionalUncertainty::NoneReported,
        Some(total) => FractionalUncertainty::Reported {
            total,
            measurement: c.real_opt(&format!("{prefix}measurement"))?,
            peculiar_velocity: c.real_opt(&format!("{prefix}velocity"))?,
        },
    })
}

/// The preferred measurement and the ranking, by the rule named `rule`, that chose it; none
/// when there is none.
pub(super) fn preferred(
    c: &Columns<'_>,
    rule: &str,
) -> Result<Option<PreferredMeasurement>, ReadError> {
    let Some(measurement) = key(c, "preferred_")? else {
        return Ok(None);
    };
    let runner_up = key(c, "runner_up_")?
        .map(|measurement| {
            Ok::<_, ReadError>(RunnerUp {
                measurement,
                uncertainty: fraction(c, "runner_up_")?,
            })
        })
        .transpose()?;
    Ok(Some(PreferredMeasurement {
        rule: rule.to_owned(),
        measurement,
        rank: c.typed("preferred_rank")?,
        distance_mpc: c.real("preferred_mpc")?,
        uncertainty: fraction(c, "preferred_")?,
        decided: c.typed("preferred_decided")?,
        runner_up,
    }))
}

/// The derived position of a placed object, from its row, its preferred measurement among
/// `measurements`, its entries and the release.
pub(super) fn derived(
    c: &Columns<'_>,
    preferred: Option<&PreferredMeasurement>,
    measurements: &[MeasurementRecord],
    entries: &[EntryRecord],
) -> Result<DerivedPositionRecord, ReadError> {
    let v: &Vocabulary = c.v;
    let preferred = preferred.ok_or_else(|| c.error("preferred_quantity", "is NULL, placed"))?;
    let key = preferred.measurement;
    let m = measurements
        .iter()
        .find(|m| (m.entry, m.quantity, m.n) == (key.entry, key.quantity, key.n))
        .ok_or_else(|| {
            c.error(
                "preferred_quantity",
                format!("the object has no measurement {key:?}"),
            )
        })?;
    // The value the position derives from: a redshift's CMB-frame value, else the native.
    let native: Quantity = match (&m.redshift, key.quantity) {
        (Some(z), MeasuredQuantity::Redshift) => z.cmb,
        _ => m.native,
    };
    let native_json = serde_json::to_value(native).map_err(|e| c.error("chain", e))?;
    let chain_id = c.int("chain")?;
    let template = v.chain(chain_id).ok_or_else(|| {
        c.error(
            "chain",
            format!("{chain_id} is not among the release's chains"),
        )
    })?;
    let params = [
        c.real_opt("chain_p1")?,
        c.real_opt("chain_p2")?,
        c.real_opt("chain_p3")?,
        c.real_opt("chain_p4")?,
    ];
    let steps = expand(template, &params, &native_json).ok_or_else(|| {
        c.error(
            "chain",
            format!("chain {chain_id} lacks a parameter it takes"),
        )
    })?;
    let chain: Vec<Step> = serde_json::from_value(steps)
        .map_err(|e| c.error("chain", format!("chain {chain_id} does not read: {e}")))?;

    let scale_domain = c.term("scale_domain")?;
    // The scale domain's unit: parsecs for stars and the Galaxy's tracers.
    let unit = crate::units::scale_domain_unit(scale_domain);
    let factor = Unit::Megaparsec
        .factor_to(unit)
        .map_err(|e| c.error("scale_domain", e))?;
    let position_entry = c.small("position_entry")?;
    let direction_frame = entries
        .iter()
        .find(|e| e.entry == position_entry)
        .and_then(|e| e.position.as_ref())
        .map(|p| p.frame)
        .ok_or_else(|| {
            c.error(
                "position_entry",
                format!("entry {position_entry} has no sky position"),
            )
        })?;
    let distance = c.quantity(c.real("distance_mpc")?, Unit::Megaparsec, "distance_")?;
    Ok(DerivedPositionRecord {
        frame: ICRS_CARTESIAN_NAME.to_owned(),
        origin: BARYCENTRE.to_owned(),
        scale_domain: scale_domain.to_owned(),
        unit,
        x: c.real("x")? * factor,
        y: c.real("y")? * factor,
        z: c.real("z")? * factor,
        epoch: c.real("epoch")?,
        direction_frame,
        distance,
        measure: c.typed("measure")?,
        native,
        assumed_cosmology: m.distance.as_ref().and_then(|d| d.assumed_cosmology),
        cosmology: v.cosmology().clone(),
        chain,
    })
}
