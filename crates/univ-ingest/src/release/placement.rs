//! Each researched structure placed once, for its `structures` and `structure_details` rows and
//! the cell index alike: its admitted grid outline where its test admitted one
//! ([`super::outline::Admitted`]), else the geometry its file gives ([`super::place`],
//! [`super::shapes`]). The centre and bounds its row records are in its scale domain's unit, pc
//! for stars and Mpc otherwise; the box the cell index holds stays in Mpc.

use std::collections::BTreeMap;

use univ_model::geometry::Aabb;
use univ_model::position::Vec3;
use univ_model::units::Unit;

use super::outline::Outlined;
use super::place::{centre, index_bounds};
use super::shapes::{PlacedShape, Placing, placed_shape};
use crate::crossmatch::MemberPlacement;
use crate::structures::Structure;
use crate::tiles::unit_of;

/// A structure as placed: its centre, bounds, shape and derivation.
#[derive(Clone, Debug)]
pub struct Placement<'a> {
    pub id: &'a str,
    pub structure: &'a Structure,
    /// Its member list as resolved, where it has one.
    pub members: Option<&'a MemberPlacement>,
    /// Its ICRS box about the barycentre in Mpc, the one the cell index holds, or why it has
    /// none.
    pub index: Result<Aabb, String>,
    /// Its ICRS centre about the barycentre in its scale domain's unit; none when unplaced.
    pub centre: Option<Vec3>,
    /// [`Self::index`] in its scale domain's unit; none when unplaced.
    pub bounds: Option<Aabb>,
    /// Its placed shape and derivation; none when unplaced.
    pub shape: Option<PlacedShape>,
}

/// Each of `structures` placed under `placing`, by its id, with the member list `members`
/// resolved for it, or as its admitted outline among `outlines`, by its id.
///
/// # Errors
///
/// Fails when a structure whose centre is placed has no shape, or its scale domain's unit is
/// not a length.
pub fn placements<'a>(
    structures: &'a [(String, Structure)],
    members: &'a [MemberPlacement],
    placing: &Placing<'_>,
    outlines: &BTreeMap<String, Outlined>,
) -> Result<Vec<Placement<'a>>, String> {
    let (release, correction) = (placing.release, placing.correction);
    structures
        .iter()
        .map(|(id, s)| {
            let placed = members.iter().find(|p| p.structure == s.name);
            let admitted = outlines.get(id).and_then(|o| o.admitted.as_ref());
            let (index, at, shape) = if let Some(a) = admitted {
                (Ok(a.bounds), Some(a.centre), Some(a.placed.clone()))
            } else {
                let at = centre(s, placed, release, correction).ok();
                let shape = match placed_shape(s, placed, placing) {
                    Ok(shape) => Some(shape),
                    // A structure without a centre is unplaced; one with a centre has a shape.
                    Err(e) if at.is_some() => {
                        return Err(format!("{id}: its centre is placed but not its shape: {e}"));
                    }
                    Err(_) => None,
                };
                (index_bounds(s, placed, release, correction), at, shape)
            };
            let per_mpc = Unit::Megaparsec
                .factor_to(unit_of(s.scale_domain))
                .map_err(|e| format!("{id}: {e}"))?;
            Ok(Placement {
                id,
                structure: s,
                members: placed,
                centre: at.map(|c| c.map(|v| v * per_mpc)),
                bounds: index.as_ref().ok().map(|b| b.scaled(per_mpc)),
                index,
                shape,
            })
        })
        .collect()
}
