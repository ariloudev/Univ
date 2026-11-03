//! The rows of the D1 schema: one Rust type per table, whose fields are the table's columns in
//! order, so a row and its table cannot drift apart. Columns holding JSON are `String`s of what
//! univ-model serializes; the build fills them, and the Worker hands them on.

use serde::{Deserialize, Serialize};

use super::sql::{Row, SqlValue, Table};

/// A field's value as an SQL value.
pub trait ToSql {
    fn to_sql(&self) -> SqlValue;
}

impl ToSql for String {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Text(self.clone())
    }
}

impl ToSql for Option<String> {
    fn to_sql(&self) -> SqlValue {
        self.clone().into()
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Real(*self)
    }
}

impl ToSql for Option<f64> {
    fn to_sql(&self) -> SqlValue {
        (*self).into()
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> SqlValue {
        (*self).into()
    }
}

impl ToSql for i64 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(*self)
    }
}

impl ToSql for u8 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(i64::from(*self))
    }
}

impl ToSql for u32 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(i64::from(*self))
    }
}

impl ToSql for u64 {
    fn to_sql(&self) -> SqlValue {
        // Counts and object ids (at most 2^53 - 1) are far below i64::MAX.
        SqlValue::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
    }
}

impl ToSql for Option<u64> {
    fn to_sql(&self) -> SqlValue {
        self.map_or(SqlValue::Null, |n| n.to_sql())
    }
}

/// A row type of `$table`, its columns named by its fields.
macro_rules! row {
    (
        $(#[$meta:meta])*
        $name:ident => $table:literal, per_release: $per:literal,
        key: [$($key:literal),*], indexes: [$([$($index:literal),*]),*],
        { $($(#[$fmeta:meta])* $field:ident: $ty:ty),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $($(#[$fmeta])* pub $field: $ty),*
        }

        impl Row for $name {
            const TABLE: Table = Table {
                name: $table,
                columns: &[$(stringify!($field)),*],
                key: &[$($key),*],
                indexes: &[$(&[$($index),*]),*],
                per_release: $per,
            };

            fn values(&self) -> Vec<SqlValue> {
                vec![$(self.$field.to_sql()),*]
            }
        }
    };
}

row! {
    /// A release as built. Publishing sets its `published_at` and pruning its `pruned_at`.
    ReleaseRow => "releases", per_release: false, key: ["number"], indexes: [], {
        number: u32,
        /// RFC 3339, UTC.
        built_at: String,
        /// JSON of the release cosmology.
        cosmology: String,
        peculiar_velocity_km_s: f64,
        model_version: String,
        ingest_version: String,
        schema_version: u32,
        /// The matching rules file, as recorded.
        matching_rules: String,
        /// The ranking rule file, as recorded.
        ranking_rule: String,
        ranking_rule_name: String,
        /// JSON array of the release's source ids.
        sources: String,
        objects: u64,
        structures: u64,
        grids: u64,
        tiles: u64,
    }
}

row! {
    /// An object with its preferred measurements and, when placed, its derived position.
    ObjectRow => "objects", per_release: true, key: ["release", "id"], indexes: [], {
        release: u32,
        id: u64,
        kind: String,
        /// Its preferred sky position, ICRS degrees, at `epoch` (a Julian year).
        ra: Option<f64>,
        dec: Option<f64>,
        epoch: Option<f64>,
        placed: bool,
        /// Why it is not placed, when it is not.
        not_placed: Option<String>,
        /// The scale domain of its preferred distance's source, when placed.
        scale_domain: Option<String>,
        /// ICRS Cartesian, Mpc, under the release cosmology.
        x: Option<f64>,
        y: Option<f64>,
        z: Option<f64>,
        distance_mpc: Option<f64>,
        preferred_distance: Option<String>,
        preferred_position: Option<String>,
        /// JSON of the derived position: frames, epoch, cosmology and transformation chain.
        position: Option<String>,
        links: String,
        relations: String,
    }
}

row! {
    /// A catalogue entry of an object.
    EntryRow => "entries", per_release: true, key: ["release", "source", "identifier"],
    indexes: [["release", "object_id"]], {
        release: u32,
        source: String,
        identifier: String,
        object_id: u64,
        anchor: bool,
        resolution: String,
        name: Option<String>,
        position: Option<String>,
        group_id: Option<String>,
        flags: String,
    }
}

row! {
    /// A measurement of an entry: `quantity` is `distance`, `redshift`, `parallax`,
    /// `velocity`, `proper_motion_ra`, `proper_motion_dec` or `magnitude`, `n` its place among
    /// the entry's values of that quantity.
    MeasurementRow => "measurements", per_release: true,
    key: ["release", "object_id", "source", "identifier", "quantity", "n"], indexes: [], {
        release: u32,
        object_id: u64,
        source: String,
        identifier: String,
        quantity: String,
        n: u32,
        kind_of_value: String,
        native_value: f64,
        native_unit: String,
        value: f64,
        unit: String,
        /// JSON of the native uncertainty.
        uncertainty: String,
        /// JSON of the whole univ-model value.
        detail: String,
    }
}

row! {
    /// A designation an object carries.
    DesignationRow => "designations", per_release: true,
    key: ["release", "source", "identifier", "object_id"], indexes: [["release", "object_id"]], {
        release: u32,
        source: String,
        identifier: String,
        object_id: u64,
    }
}

row! {
    /// A common name of an object, by its key (see [`name_key`]).
    NameRow => "names", per_release: true, key: ["release", "key", "object_id", "source"],
    indexes: [], {
        release: u32,
        key: String,
        object_id: u64,
        source: String,
        name: String,
    }
}

row! {
    /// The ids a release issued.
    IdsIssuedRow => "ids_issued", per_release: false, key: ["release"], indexes: [], {
        release: u32,
        first: u64,
        last: u64,
    }
}

row! {
    /// An id that left the releases, in `release`.
    IdReplacedRow => "ids_replaced", per_release: false, key: ["id"], indexes: [], {
        id: u64,
        release: u32,
        /// `merged_into`, `split_into` or `retired`.
        change: String,
        /// JSON array of the ids that replaced it, ascending.
        replaced_by: String,
    }
}

row! {
    /// A placed object in its level-0 cell.
    ObjectCellRow => "object_cells", per_release: true,
    key: ["release", "domain", "ix", "iy", "iz", "object_id"], indexes: [], {
        release: u32,
        domain: String,
        ix: i64,
        iy: i64,
        iz: i64,
        object_id: u64,
    }
}

row! {
    /// A structure in one cell its bounding box meets.
    StructureCellRow => "structure_cells", per_release: true,
    key: ["release", "level", "ix", "iy", "iz", "structure"], indexes: [], {
        release: u32,
        level: u8,
        ix: i64,
        iy: i64,
        iz: i64,
        structure: String,
    }
}

row! {
    /// A source of the release with its citation, terms and query.
    SourceRow => "sources", per_release: true, key: ["release", "id"], indexes: [], {
        release: u32,
        id: String,
        title: String,
        provider: String,
        scale_domain: String,
        citation: String,
        terms_url: String,
        terms_quote: String,
        credit: String,
        /// `permitted` or `permitted_with_credit`.
        redistribution: String,
        /// `provisional` or `settled`.
        review: String,
        /// Catalogue entries the release holds from it.
        entries: u64,
        /// JSON of the access method and query, verbatim.
        access: String,
        cuts: String,
        fixes: String,
    }
}

row! {
    /// A payload a source's provenance records.
    PayloadRow => "payloads", per_release: true, key: ["release", "source", "item"],
    indexes: [], {
        release: u32,
        source: String,
        item: String,
        url: String,
        sha256: String,
        bytes: u64,
        retrieved: String,
        rows: Option<u64>,
    }
}

row! {
    /// What normalization made of a source's rows.
    NormalizationRow => "normalizations", per_release: true, key: ["release", "source"],
    indexes: [], {
        release: u32,
        source: String,
        published: u64,
        /// JSON map of the entries left out by reason.
        left_out: String,
        fixes: String,
    }
}

row! {
    /// A researched structure: its record as its file gives it, every number cited.
    StructureRow => "structures", per_release: true, key: ["release", "id"], indexes: [], {
        release: u32,
        id: String,
        name: String,
        kind: String,
        status: String,
        geometry_kind: Option<String>,
        record: String,
        /// JSON of its member list as resolved at ingest.
        members: Option<String>,
        /// JSON of its ICRS bounding box in Mpc, when it has one.
        bounds: Option<String>,
    }
}

row! {
    /// A scale domain's tiles.
    ScaleDomainRow => "scale_domains", per_release: true, key: ["release", "name"], indexes: [], {
        release: u32,
        name: String,
        unit: String,
        radius: f64,
        points: u64,
        nodes: u64,
        manifest_key: String,
    }
}

row! {
    /// A volume grid.
    GridRow => "grids", per_release: true, key: ["release", "id"], indexes: [], {
        release: u32,
        id: String,
        source: String,
        unit: String,
        shape: String,
        fields: String,
        header_key: String,
    }
}

/// Every table a release writes rows to, in the order its D1 files load them.
pub const TABLES: [Table; 16] = [
    ReleaseRow::TABLE,
    SourceRow::TABLE,
    PayloadRow::TABLE,
    NormalizationRow::TABLE,
    ObjectRow::TABLE,
    EntryRow::TABLE,
    MeasurementRow::TABLE,
    DesignationRow::TABLE,
    NameRow::TABLE,
    IdsIssuedRow::TABLE,
    IdReplacedRow::TABLE,
    StructureRow::TABLE,
    ScaleDomainRow::TABLE,
    GridRow::TABLE,
    ObjectCellRow::TABLE,
    StructureCellRow::TABLE,
];

/// The key a common name is looked up by: lower case, without white space.
#[must_use]
pub fn name_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
