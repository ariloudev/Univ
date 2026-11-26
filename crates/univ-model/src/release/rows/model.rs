//! The row of the Galaxy model a release sampled (schema version 9): its definition as it was
//! sampled, beside the scale domain its model points are tiled in.

use serde::{Deserialize, Serialize};

use super::{Json, ToSql};
use crate::release::sql::{Row, Scale, Scope, SqlValue, Table};

row! {
    /// The Galaxy model a release sampled into its scale domain `domain` (build-the-galaxy-model):
    /// its points and seed, its definition, [`crate::galaxy_model::Model`] as JSON, whose every
    /// number gives its citation or its reason as this project's choice and whose populations
    /// are named by the codes their model points carry in the tiles, and the ids of the
    /// structures of the arms it followed, a JSON list.
    GalaxyModelRow => "galaxy_models", scope: PerRelease, scale: Release, key: ["domain", "release"],
    indexes: [], {
        release: u32,
        /// The scale domain of its model points: `galaxy_model`.
        domain: String,
        points: u64,
        seed: u64,
        definition: Json,
        followed: Json,
    }
}
