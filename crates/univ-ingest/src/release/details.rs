//! A release's `structure_details` rows: each researched structure's placed shape and its
//! derivation ([`super::placement`]: its file's geometry placed by [`super::shapes`], or its
//! admitted grid outline, [`super::outline`]), the test of its outline on a volume grid and its
//! lengths rescaled to the release cosmology ([`super::rescaled`]).

use std::collections::BTreeMap;

use univ_model::derive::Release;
use univ_model::release::rows::StructureDetailRow;

use super::objects::json;
use super::outline::{Outlined, grid_missing, not_drawn};
use super::placement::Placement;
use super::rescaled::rescaled;

/// The `structure_details` rows of release `release_number`: each structure as `placed`, with
/// the record of its outline among `outlines`, by its id, where the build recorded its outline,
/// and its lengths rescaled to `release`.
///
/// # Errors
///
/// Fails when a length cannot be rescaled or a value cannot be written as JSON.
pub fn structure_detail_rows(
    release_number: u32,
    placed: &[Placement<'_>],
    outlines: &BTreeMap<String, Outlined>,
    release: &Release,
) -> Result<Vec<StructureDetailRow>, String> {
    placed
        .iter()
        .map(|p| {
            let (id, s) = (p.id, p.structure);
            let outline = match (outlines.get(id), &s.outline) {
                (Some(o), _) => Some(o.record.clone()),
                (None, Some(record)) => Some(not_drawn(record, &grid_missing(record))?),
                (None, None) => None,
            };
            let rescaled = rescaled(s, release).map_err(|e| format!("{id}: {e}"))?;
            Ok(StructureDetailRow {
                release: release_number,
                id: id.to_owned(),
                shape: p.shape.as_ref().map(|p| json(&p.shape)).transpose()?,
                derivation: p.shape.as_ref().map(|p| json(&p.derivation)).transpose()?,
                outline: outline.as_ref().map(json).transpose()?,
                rescaled: (!rescaled.is_empty())
                    .then(|| json(&rescaled))
                    .transpose()?,
            })
        })
        .collect()
}
