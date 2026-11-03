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

use univ_model::api::structures::{Length, NativeLength, RescaledLength, RescalingMethod};
use univ_model::cosmology::AssumedCosmology;
use univ_model::derive::{Release, Rescaling};
use univ_model::units::{Dimension, Unit};

use super::place::assumed;
use super::pointers;
use crate::crossmatch::STRUCTURE_REDSHIFTS;
use crate::structures::{Cited, Structure};

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

/// The item of the length `c` at `pointer`, published for `a` and rescaled by `rescaling`.
fn item(
    pointer: &str,
    c: &Cited,
    a: AssumedCosmology,
    rescaling: &Rescaling,
) -> Result<RescaledLength, String> {
    let r = rescaling
        .apply(&c.quantity)
        .map_err(|e| format!("{pointer} cannot be rescaled: {e}"))?;
    let stated = c
        .assumes
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| format!("{pointer}: its cosmology cannot be written as JSON: {e}"))?;
    Ok(RescaledLength {
        value: pointer.to_owned(),
        native: NativeLength {
            value: r.native.value,
            unit: r.native.unit,
            hubble_constant: a.hubble_constant,
            assumed: stated,
        },
        rescaled: Length {
            value: r.rescaled.value,
            unit: r.rescaled.unit,
        },
        factor: r.factor,
        method: if rescaling.redshift.is_some() {
            RescalingMethod::ComovingRatio
        } else {
            RescalingMethod::HubbleRatio
        },
        redshift: rescaling.redshift,
    })
}

/// The structure's lengths published under another cosmology, each as published and rescaled to
/// `release`'s; empty when it has none.
///
/// # Errors
///
/// Fails when a length's cosmology cannot stand or it cannot be rescaled.
pub fn rescaled(s: &Structure, release: &Release) -> Result<Vec<RescaledLength>, String> {
    // Each length with the redshift its rule rescales at: none, the ratio of Hubble constants,
    // for those that place the structure; its cited redshift for its other values.
    let mut lengths: Vec<(String, &Cited, Option<f64>)> = pointers::position(s)
        .into_iter()
        .chain(s.geometry.iter().flat_map(pointers::geometry))
        .map(|(pointer, c)| (pointer, c, None))
        .collect();
    let z = redshift(s);
    lengths.extend(
        s.values
            .iter()
            .map(|(name, c)| (pointers::value(name), c, z)),
    );
    lengths
        .into_iter()
        .filter(|(_, c, _)| is_length(c.unit()))
        .filter_map(|(pointer, c, at)| Some((pointer, c, assumed(c)?, at)))
        .map(|(pointer, c, a, at)| {
            let rule = match at {
                Some(z) => release.rescaling_at(a, z),
                None => release.hubble_rescaling(a),
            };
            item(&pointer, c, a, &rule.map_err(|e| e.to_string())?)
        })
        .collect()
}
