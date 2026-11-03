//! The dense places a void may not hold, as the earlier Univ void outlines checked: a region
//! below its threshold that takes in a galaxy cluster or a supercluster is not a void, whatever
//! the galaxy counts say. Which places count is the outline record's rule, a choice of this
//! pipeline.

use std::collections::HashSet;

use serde::Serialize;
use univ_model::grid::GridGeometry;
use univ_model::object::Object;
use univ_model::position::{SkyPosition, Vec3, from_icrs};

use super::region::{Region, on_grid, velocity_distance};
use crate::structures::{Coordinates, DensePlaces, Structure};

/// A dense place, where it was published.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DensePlace {
    pub name: String,
    pub at: DenseAt,
}

/// Where a dense place is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "from")]
pub enum DenseAt {
    /// A catalogue entry's ICRS direction and CMB-frame redshift.
    Entry {
        icrs: SkyPosition,
        cmb_redshift: f64,
    },
    /// A researched structure's published position.
    Published(Box<Coordinates>),
}

/// What the dense-place check of an outline found.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DenseCheck {
    pub rule: DensePlaces,
    /// Dense places given to the check, and those of them that lie on the grid.
    pub given: usize,
    pub on_grid: usize,
    /// The dense places the region holds, by name.
    pub held: Vec<String>,
    /// The dense places that could not be put on the grid, each with why: `given − on_grid`
    /// of them, none dropped unsaid.
    pub off_grid: Vec<OffGrid>,
}

/// A dense place the check could not put on the grid, and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OffGrid {
    pub name: String,
    pub reason: String,
}

/// The dense places of `rule` among `objects` and `structures`: every object holding an entry
/// of one of the rule's sources, at that entry's preferred CMB-frame redshift (an object whose
/// entry has none is left out), and every structure of one of its kinds with a published position.
#[must_use]
pub fn dense_places<'a>(
    rule: &DensePlaces,
    objects: impl IntoIterator<Item = &'a Object>,
    structures: impl IntoIterator<Item = &'a Structure>,
) -> Vec<DensePlace> {
    let mut places = Vec::new();
    for object in objects {
        let found = object.entries.iter().find_map(|e| {
            let entry = &e.entry;
            if !rule.sources.contains(&entry.designation.source) {
                return None;
            }
            let z = entry.preferred_redshift()?;
            Some((entry, entry.position.as_ref()?, z.cmb.value.value))
        });
        if let Some((entry, position, cmb_redshift)) = found {
            let name = object.names.first().map_or_else(
                || {
                    format!(
                        "{} {}",
                        entry.designation.source, entry.designation.identifier
                    )
                },
                |n| n.name.clone(),
            );
            places.push(DensePlace {
                name,
                at: DenseAt::Entry {
                    icrs: position.icrs,
                    cmb_redshift,
                },
            });
        }
    }
    for s in structures {
        if let (true, Some(position)) = (rule.structure_kinds.contains(&s.kind), &s.position) {
            places.push(DensePlace {
                name: s.name.clone(),
                at: DenseAt::Published(Box::new(position.clone())),
            });
        }
    }
    places
}

/// Where `place` lies in velocity space on a grid of `geometry`, h⁻¹ Mpc in its frame, or why
/// it cannot be put there.
fn on(place: &DensePlace, geometry: &GridGeometry) -> Result<Vec3, String> {
    match &place.at {
        DenseAt::Entry { icrs, cmb_redshift } => {
            let r = velocity_distance(*cmb_redshift);
            Ok(from_icrs(geometry.frame)
                .apply(icrs.unit_vector())
                .map(|u| u * r))
        }
        DenseAt::Published(position) => on_grid(position, geometry),
    }
}

/// The grid sample `place` falls in, or why it falls in none.
fn sample_of(place: &DensePlace, geometry: &GridGeometry) -> Result<usize, String> {
    let at = on(place, geometry)?;
    let cell = geometry
        .cell_at(at)
        .ok_or_else(|| format!("it lies at {at:?} h^-1 Mpc, outside the grid"))?;
    geometry
        .sample_index(cell)
        .ok_or_else(|| "its cell holds no sample of the grid".to_owned())
}

/// The check of `region` on a grid of `geometry` against the dense places of `rule`.
#[must_use]
pub fn check(
    rule: &DensePlaces,
    places: &[DensePlace],
    geometry: &GridGeometry,
    region: &Region,
) -> DenseCheck {
    let inside: HashSet<usize> = region
        .cells
        .iter()
        .filter_map(|c| geometry.sample_index(*c))
        .collect();
    let mut on_grid = 0;
    let mut held = Vec::new();
    let mut off_grid = Vec::new();
    for place in places {
        let i = match sample_of(place, geometry) {
            Ok(i) => i,
            Err(reason) => {
                off_grid.push(OffGrid {
                    name: place.name.clone(),
                    reason,
                });
                continue;
            }
        };
        on_grid += 1;
        if inside.contains(&i) {
            held.push(place.name.clone());
        }
    }
    DenseCheck {
        rule: rule.clone(),
        given: places.len(),
        on_grid,
        held,
        off_grid,
    }
}
