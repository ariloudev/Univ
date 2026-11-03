//! Each length a structure file publishes under another cosmology, as published beside the length
//! rescaled to the release cosmology, with the factor and the rule (univ-model's
//! `Release::hubble_rescaling` and `Release::rescaling_at`; nothing is derived here).
//!
//! A length counts when it is in h⁻¹ Mpc (published for H0 = 100) or its `assumes` states a
//! Hubble constant; one whose `assumes` states none is taken as published and not listed.
//!
//! - **The lengths that place the structure**, those of its position and geometry, are rescaled
//!   as [`super::place`] rescales them: by the ratio of Hubble constants (`hubble_ratio`).
//! - **Its other values**, such as extents and diameters, are rescaled at the structure's cited
//!   redshift (its value `redshift` or `central_redshift`) by the ratio of comoving distances
//!   when their cosmology is a flat ΛCDM (`comoving_ratio`), which keeps a size in proportion to
//!   its distance at any redshift; otherwise, or without a redshift, by the ratio of Hubble
//!   constants.

use serde_json::{Value as Json, json};
use univ_model::derive::{Release, Rescaling};
use univ_model::units::Unit;

use super::objects::snake;
use super::place::assumed;
use super::pointers::{self, Pointed};
use crate::crossmatch::STRUCTURE_REDSHIFTS;
use crate::structures::{Cited, Structure};
use univ_model::units::Dimension;

/// Whether `unit` measures a length, plain or in h⁻¹ Mpc.
fn is_length(unit: Unit) -> bool {
    matches!(
        unit.dimension(),
        Dimension::Length | Dimension::HubbleScaledLength
    )
}

/// The structure's cited redshift, the one its member tracers fall back on: its value
/// `redshift` or `central_redshift`.
fn redshift(s: &Structure) -> Option<f64> {
    STRUCTURE_REDSHIFTS.iter().find_map(|key| {
        let c = s.values.get(*key)?;
        (c.unit() == Unit::Dimensionless).then(|| c.value())
    })
}

/// One rescaled length's item.
fn item(
    pointer: &str,
    c: &Cited,
    rescaling: Result<Rescaling, String>,
) -> Result<Option<Json>, String> {
    let Some(a) = assumed(c) else {
        return Ok(None);
    };
    let rescaling = rescaling?;
    let r = rescaling
        .apply(&c.quantity)
        .map_err(|e| format!("{pointer} cannot be rescaled: {e}"))?;
    let method = if rescaling.redshift.is_some() {
        "comoving_ratio"
    } else {
        "hubble_ratio"
    };
    Ok(Some(json!({
        "value": pointer,
        "native": {
            "value": r.native.value,
            "unit": snake(&r.native.unit),
            "hubble_constant": a.hubble_constant,
            "assumed": c.assumes,
        },
        "rescaled": {"value": r.rescaled.value, "unit": snake(&r.rescaled.unit)},
        "factor": r.factor,
        "method": method,
        "redshift": rescaling.redshift,
    })))
}

/// The structure's lengths published under another cosmology, each as published and rescaled to
/// `release`'s; none when it has none.
///
/// # Errors
///
/// Fails when a length's cosmology cannot stand or it cannot be rescaled.
pub fn rescaled(s: &Structure, release: &Release) -> Result<Option<Json>, String> {
    let mut placing: Vec<Pointed<'_>> = pointers::position(s);
    if let Some(g) = &s.geometry {
        placing.extend(pointers::geometry(g));
    }
    placing.retain(|(_, c)| is_length(c.unit()));
    let mut items = Vec::new();
    for (pointer, c) in placing {
        let rule = assumed(c).map(|a| release.hubble_rescaling(a).map_err(|e| e.to_string()));
        if let Some(rule) = rule {
            items.extend(item(&pointer, c, rule)?);
        }
    }
    let z = redshift(s);
    for (name, c) in &s.values {
        if !is_length(c.unit()) {
            continue;
        }
        let Some(a) = assumed(c) else { continue };
        let rule = match z {
            Some(z) => release.rescaling_at(a, z),
            None => release.hubble_rescaling(a),
        };
        items.extend(item(
            &pointers::value(name),
            c,
            rule.map_err(|e| e.to_string()),
        )?);
    }
    Ok((!items.is_empty()).then_some(Json::Array(items)))
}
