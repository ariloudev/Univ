//! The rows of how a release's sources are read beyond their row in `sources`: the conditions
//! of their terms and the uncertainties they state once (schema version 4), and their
//! designation forms, ranks, flag meanings and share-alike clauses (schema version 7).

use serde::{Deserialize, Serialize};

use super::{Json, ToSql};
use crate::release::sql::{Row, Scale, Scope, SqlValue, Table};

row! {
    /// The conditions of a source's terms beyond its status, and the uncertainties it states
    /// once for its whole catalogue, beside its row in `sources` (schema version 4): written
    /// only for a source that has one of them.
    SourceTermRow => "source_terms", scope: PerRelease, scale: Source, key: ["source", "release"],
    indexes: [], {
        release: u32,
        /// The source's `sources.n`.
        source: u32,
        /// The clause of its terms that allows only non-commercial use, quoted; none when its
        /// terms allow commercial use.
        non_commercial: Option<String>,
        /// JSON of the request for permission to redistribute it,
        /// [`crate::api::sources::Permission`], while its status is `permission_pending`.
        permission: Option<Json>,
        /// JSON array of the uncertainties it states once for its whole catalogue, each a
        /// [`CatalogueUncertainty`]; none when it has none.
        catalogue_uncertainties: Option<Json>,
    }
}

row! {
    /// How a source's own values are read, beside its row in `sources` (schema version 7):
    /// written only for a source that gives a designation form, flag meanings or a share-alike
    /// clause, or that the designation rules rank.
    SourceReadingRow => "source_reading", scope: PerRelease, scale: Source, key: ["source", "release"],
    indexes: [], {
        release: u32,
        /// The source's `sources.n`.
        source: u32,
        /// The form its catalogue writes a designation in, holding `{identifier}` once.
        designation_form: Option<String>,
        /// The catalogue documentation the form is read from.
        designation_cite: Option<String>,
        /// Its place in the designation rules, 1 first; none where they do not list it.
        rank: Option<u32>,
        /// The clause of its terms requiring data adapted from it to be shared under the same
        /// licence, quoted; none when its terms require no such thing.
        share_alike: Option<String>,
        /// JSON of what each value of its flag columns means, by column: `{"values", "cite"}`.
        flags: Option<Json>,
    }
}

/// An uncertainty a source states once for its whole catalogue rather than for each row, such
/// as 2dFGRS's rms of 85 km/s or a photometric redshift scatter σz / (1 + z): as a source
/// definition writes it, given to each value as a symmetric error, estimated, citing `basis`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueError {
    pub value: f64,
    /// Its unit: the column's, or a velocity for a column of z (σz = σv / c).
    pub unit: crate::units::Unit,
    /// Whether it is stated per unit of (1 + z), as σz / (1 + z) is.
    #[serde(default)]
    pub times_one_plus_z: bool,
    /// Where the source states it, quoted or cited.
    pub basis: String,
}

impl CatalogueError {
    /// The error in the unit of a column in `column`, before any factor of (1 + z): none when
    /// the units cannot meet.
    #[must_use]
    pub fn in_unit(&self, column: crate::units::Unit) -> Option<f64> {
        use crate::units::{Dimension, Unit};
        if column.dimension() == Dimension::Dimensionless
            && self.unit.dimension() == Dimension::Velocity
        {
            let km_s = self.unit.factor_to(Unit::KilometrePerSecond).ok()?;
            return Some(self.value * km_s / crate::cosmology::SPEED_OF_LIGHT);
        }
        Some(self.value * self.unit.factor_to(column).ok()?)
    }
}

/// A [`CatalogueError`] with the quantity whose every value from the source takes it as an
/// estimated uncertainty, as `source_terms` holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogueUncertainty {
    pub quantity: crate::api::objects::MeasuredQuantity,
    #[serde(flatten)]
    pub error: CatalogueError,
}
