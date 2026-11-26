//! The Galaxy model of a release as Univ Server gives it (read-the-galaxy-model): its
//! definition as the release sampled it, with every number's citation or its reason as this
//! project's choice, and where its model points are tiled.

use serde::{Deserialize, Serialize};

use crate::galaxy_model::Model;

/// The Galaxy model a release holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GalaxyModel {
    pub release: u32,
    /// The scale domain its model points are tiled in: `galaxy_model`.
    pub domain: String,
    /// How many model points it was sampled into and the seed its sampling was drawn with
    /// (`points`, `seed`); its populations, each with the code its model points carry in the tiles, its density,
    /// light and display colour; the spiral arms' rules; and the papers its numbers cite. Each
    /// number gives its value, unit, kind of value, uncertainty and its citation or the reason
    /// for it.
    pub definition: Model,
    /// The ids of the structures of the arms it followed, in the order of its arm rules.
    pub followed: Vec<String>,
}
