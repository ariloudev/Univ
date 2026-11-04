//! A researched structure's member list resolved at ingest: each member the paper names is
//! looked up among the matched objects, and a list whose members are all placed becomes a
//! univ-model member list with each member's position and the structure's bounding box cached
//! with it. A structure with a member that resolves to no object, or to one that is not placed,
//! keeps its published position as its point, and the members are reported.
//!
//! How a member resolves, a choice of this pipeline:
//!
//! - Its name in the paper's table, trimmed, with runs of white space as one space and letters
//!   compared without case, must equal a designation's identifier or a common name of exactly
//!   one matched object. A name without a letter, such as a row number, resolves to nothing: a
//!   bare number names no catalogue.
//! - A member that no object carries is placed from the structure file itself, at its own
//!   cited sky position and redshift ([`super::tracers`]), as a tracer, and recorded with the
//!   values that placed it.
//! - The object's preferred sky position must lie within the matching rules'
//!   `structure_members.angular_tolerance_arcsec` of the member's published position, so that
//!   a name shared by two catalogues cannot pull in an object elsewhere on the sky. Where the
//!   check cannot be made, the member published without a usable position or the object
//!   without a sky position, the member is resolved by its name alone, recorded as unchecked
//!   and counted.

use std::collections::HashMap;

use serde::Serialize;
use univ_model::derive::DerivedPosition;
use univ_model::geometry::{Aabb, Geometry, Member, Shape};
use univ_model::ids::ObjectId;
use univ_model::object::{NotPlacedReason, Object, Placement};
use univ_model::position::{Frame, Vec3, between, norm, separation_degrees, unit_vector};
use univ_model::units::Unit;

use super::tracers::{FilePlacer, FromFile};
use crate::structures::{ColumnName, Coordinates, Members, Row, Structure, StructureGeometry};

/// A matched object as a member list sees it: its id, the object and its derived position.
pub struct Resolved<'a> {
    pub id: ObjectId,
    pub object: &'a Object,
    pub position: Option<&'a DerivedPosition>,
}

/// A structure's member list after resolution.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemberPlacement {
    pub structure: String,
    pub members: usize,
    /// The member list, in the ICRS and Mpc, when every member is placed.
    pub shape: Option<Shape>,
    /// The member list's bounding box in the ICRS, Mpc, cached with it.
    pub bounds: Option<Aabb>,
    /// Each member that cannot be placed, and why.
    pub unplaced: Vec<UnplacedMember>,
    /// The members resolved by name alone, their position unchecked against the object's.
    pub by_name_only: Vec<String>,
    /// The published position the structure is shown at when its members cannot all be placed.
    pub point: Option<Coordinates>,
    /// The members no object carries, placed from the structure file's own sky position and
    /// redshift for each, as tracers.
    pub from_file: Vec<FromFile>,
}

/// A member that cannot be placed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnplacedMember {
    pub member: String,
    pub why: MemberProblem,
}

/// Why a member cannot be placed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "is")]
pub enum MemberProblem {
    /// No matched object carries its name.
    NoObject,
    /// Several matched objects carry its name.
    Ambiguous { objects: usize },
    /// The object carrying its name lies too far from the member's published position.
    TooFar { id: ObjectId, arcsec: f64 },
    /// It resolves to an object that is not placed.
    Unplaced {
        id: ObjectId,
        reason: NotPlacedReason,
    },
}

impl MemberPlacement {
    /// Members that name no matched object, and members that name one not placed.
    #[must_use]
    pub fn unresolved_and_unplaced(&self) -> (u64, u64) {
        let unplaced = self
            .unplaced
            .iter()
            .filter(|m| matches!(m.why, MemberProblem::Unplaced { .. }))
            .count() as u64;
        (self.unplaced.len() as u64 - unplaced, unplaced)
    }
}

/// A name as members are compared: trimmed, white space collapsed, without case; none for a
/// name without a letter.
fn key(name: &str) -> Option<String> {
    name.chars().any(char::is_alphabetic).then(|| {
        name.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    })
}

/// Every matched object by each name it carries.
pub struct NameIndex<'a> {
    by_name: HashMap<String, Vec<usize>>,
    objects: &'a [Resolved<'a>],
}

impl<'a> NameIndex<'a> {
    #[must_use]
    pub fn new(objects: &'a [Resolved<'a>]) -> Self {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in objects.iter().enumerate() {
            let names = c.object.designations.iter().map(|d| d.identifier.as_str());
            let common = c.object.names.iter().map(|n| n.name.as_str());
            for name in names.chain(common).filter_map(key) {
                let found = by_name.entry(name).or_default();
                if found.last() != Some(&i) {
                    found.push(i);
                }
            }
        }
        Self { by_name, objects }
    }
}

/// Resolves the member list of each structure that has one, a member's object within
/// `tolerance_arcsec` of its published position; a member no object carries is placed by
/// `placer` from the structure file's own sky position and redshift for it, where it cites
/// them, and is not placed without a placer.
#[must_use]
pub fn resolve_all(
    structures: &[Structure],
    index: &NameIndex<'_>,
    tolerance_arcsec: f64,
    placer: Option<&FilePlacer<'_>>,
) -> Vec<MemberPlacement> {
    structures
        .iter()
        .filter_map(|s| match &s.geometry {
            Some(StructureGeometry::Members(m)) => {
                Some(resolve(s, m, index, tolerance_arcsec, placer))
            }
            _ => None,
        })
        .collect()
}

fn resolve(
    structure: &Structure,
    members: &Members,
    index: &NameIndex<'_>,
    tolerance_arcsec: f64,
    from_file_placer: Option<&FilePlacer<'_>>,
) -> MemberPlacement {
    let to_icrs = between(members.frame, Frame::Icrs);
    let degrees = |name: ColumnName| {
        let i = members.column(name)?;
        let factor = members.columns[i].unit.factor_to(Unit::Degree).ok()?;
        Some((i, factor))
    };
    let (lon, lat) = (degrees(ColumnName::Lon), degrees(ColumnName::Lat));
    let cartesian = [ColumnName::X, ColumnName::Y, ColumnName::Z].map(|name| members.column(name));
    let mut placed = Vec::new();
    let mut unplaced = Vec::new();
    let mut by_name_only = Vec::new();
    let mut from_file = Vec::new();
    for row in &members.rows {
        let published = lon
            .zip(lat)
            .map(|((i, fi), (j, fj))| unit_vector(row.values[i] * fi, row.values[j] * fj))
            .or_else(|| direction(cartesian, row))
            .map(|u| to_icrs.apply(u));
        match member(&row.name, published, index, tolerance_arcsec) {
            Ok((m, Checked::Position)) => placed.push(m),
            Ok((m, Checked::NameOnly)) => {
                by_name_only.push(row.name.clone());
                placed.push(m);
            }
            Err(MemberProblem::NoObject)
                if let Some((at, record)) = from_file_placer
                    .zip(published)
                    .and_then(|(p, toward)| p.place(structure, members, row, toward)) =>
            {
                placed.push(Member::Tracer {
                    at,
                    kind: members.tracer.clone(),
                });
                from_file.push(record);
            }
            Err(why) => unplaced.push(UnplacedMember {
                member: row.name.clone(),
                why,
            }),
        }
    }
    let shape = if unplaced.is_empty() {
        let geometry = Geometry::Members { members: placed };
        Shape::new(Frame::Icrs, Unit::Megaparsec, geometry).ok()
    } else {
        None
    };
    MemberPlacement {
        structure: structure.name.clone(),
        members: members.rows.len(),
        bounds: shape.as_ref().and_then(|s| s.bounds_in(Frame::Icrs)),
        point: shape
            .is_none()
            .then(|| structure.position.clone())
            .flatten(),
        shape,
        unplaced,
        by_name_only,
        from_file,
    }
}

/// The direction of `row`'s Cartesian position, from its `x`, `y` and `z` columns, in the
/// table's frame; none without them, or at the origin.
fn direction(columns: [Option<usize>; 3], row: &Row) -> Option<Vec3> {
    let [x, y, z] = columns;
    let position = [row.values[x?], row.values[y?], row.values[z?]];
    let length = norm(position);
    (length > 0.0).then(|| position.map(|c| c / length))
}

/// Whether a resolved member's position was checked against its object's.
enum Checked {
    Position,
    NameOnly,
}

/// The member named `name`, published toward `published` (an ICRS unit vector), as an object
/// member at its derived position, and whether its position could be checked.
fn member(
    name: &str,
    published: Option<Vec3>,
    index: &NameIndex<'_>,
    tolerance_arcsec: f64,
) -> Result<(Member, Checked), MemberProblem> {
    let found = key(name)
        .and_then(|k| index.by_name.get(&k))
        .map_or(&[][..], Vec::as_slice);
    let c = match found {
        [] => return Err(MemberProblem::NoObject),
        [one] => &index.objects[*one],
        many => {
            return Err(MemberProblem::Ambiguous {
                objects: many.len(),
            });
        }
    };
    let sky = c
        .object
        .preferred_position
        .as_ref()
        .and_then(|p| c.object.entry(&p.entry))
        .and_then(|e| e.entry.position.as_ref())
        .map(|p| p.icrs.unit_vector());
    let checked = if let (Some(a), Some(b)) = (sky, published) {
        let arcsec = separation_degrees(a, b) * 3_600.0;
        if arcsec > tolerance_arcsec {
            return Err(MemberProblem::TooFar { id: c.id, arcsec });
        }
        Checked::Position
    } else {
        Checked::NameOnly
    };
    let reason = match c.object.placement {
        Placement::NotPlaced { reason } => reason,
        Placement::Placed => NotPlacedReason::NotDerivable,
    };
    let at = c
        .position
        .and_then(|p| p.cartesian(Frame::Icrs))
        .ok_or(MemberProblem::Unplaced { id: c.id, reason })?;
    Ok((Member::Object { id: c.id, at }, checked))
}
