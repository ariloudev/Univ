//! A release's `structure_details` rows: each researched structure's placed shape and its
//! derivation ([`super::shapes`]), the test of its outline on a volume grid
//! ([`super::outline`]) and its lengths rescaled to the release cosmology
//! ([`super::rescaled`]).

use std::collections::BTreeMap;

use serde_json::Value as Json;
use univ_model::release::rows::StructureDetailRow;

use super::objects::json;
use super::outline::{grid_missing, not_drawn};
use super::place::centre;
use super::rescaled::rescaled;
use super::shapes::{Placing, placed_shape};
use crate::crossmatch::MemberPlacement;
use crate::structures::Structure;

/// The `structure_details` rows of release `release_number`: each of `structures` by its id,
/// placed under `placing` with the member list `placements` resolved for it, and the `outline`
/// column the build recorded for it in `outlines`, by its id.
///
/// # Errors
///
/// Fails when a structure whose centre is placed has no shape, a length cannot be rescaled, or a
/// value cannot be written as JSON.
pub fn structure_detail_rows(
    release_number: u32,
    structures: &[(String, Structure)],
    placements: &[MemberPlacement],
    placing: &Placing<'_>,
    outlines: &BTreeMap<String, Json>,
) -> Result<Vec<StructureDetailRow>, String> {
    structures
        .iter()
        .map(|(id, s)| {
            let placed = placements.iter().find(|p| p.structure == s.name);
            let shape = match placed_shape(s, placed, placing) {
                Ok(shape) => Some(shape),
                // A structure without a centre is unplaced; one with a centre has a shape.
                Err(e) if centre(s, placed, placing.release, placing.correction).is_ok() => {
                    return Err(format!("{id}: its centre is placed but not its shape: {e}"));
                }
                Err(_) => None,
            };
            let outline = s.outline.as_ref().map(|record| {
                outlines
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| not_drawn(record, &grid_missing(record)))
            });
            let rescaled = rescaled(s, placing.release).map_err(|e| format!("{id}: {e}"))?;
            Ok(StructureDetailRow {
                release: release_number,
                id: id.clone(),
                shape: shape.as_ref().map(|p| json(&p.shape)).transpose()?,
                derivation: shape.as_ref().map(|p| json(&p.derivation)).transpose()?,
                outline: outline.as_ref().map(json).transpose()?,
                rescaled: rescaled.as_ref().map(json).transpose()?,
            })
        })
        .collect()
}
