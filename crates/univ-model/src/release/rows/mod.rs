//! The rows of the D1 schema: one Rust type per table, whose fields are the table's columns in
//! order, so a row and its table cannot drift apart. Columns holding JSON are `String`s of what
//! univ-model serializes; the build fills them, and the Worker hands them on. A term column
//! holds a [`TermRow`] id, a `source` column a [`SourceRow`]'s `n` (see the schema).

/// A row type of `$table`, its columns named by its fields.
macro_rules! row {
    (
        $(#[$meta:meta])*
        $name:ident => $table:literal, scope: $scope:ident,
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
                kinds: &[$(<$ty as ToSql>::KIND),*],
                key: &[$($key),*],
                indexes: &[$(&[$($index),*]),*],
                scope: Scope::$scope,
            };

            fn values(&self) -> Vec<SqlValue> {
                vec![$(self.$field.to_sql()),*]
            }
        }
    };
}

mod objects;

pub use objects::{ChainRow, DesignationRow, EntryRow, MeasurementRow, NameRow, ObjectRow, PARAMS};

use serde::{Deserialize, Serialize};

use super::sql::{ColumnKind, Row, Scope, SqlValue, Table};

/// A field's value as an SQL value, and the type of its column.
pub trait ToSql {
    const KIND: ColumnKind;
    fn to_sql(&self) -> SqlValue;
}

impl ToSql for String {
    const KIND: ColumnKind = ColumnKind::Text;

    fn to_sql(&self) -> SqlValue {
        SqlValue::Text(self.clone())
    }
}

impl ToSql for Option<String> {
    const KIND: ColumnKind = ColumnKind::Text;

    fn to_sql(&self) -> SqlValue {
        self.clone().into()
    }
}

impl ToSql for f64 {
    const KIND: ColumnKind = ColumnKind::Real;

    fn to_sql(&self) -> SqlValue {
        SqlValue::Real(*self)
    }
}

impl ToSql for Option<f64> {
    const KIND: ColumnKind = ColumnKind::Real;

    fn to_sql(&self) -> SqlValue {
        (*self).into()
    }
}

impl ToSql for bool {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        (*self).into()
    }
}

impl ToSql for i64 {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(*self)
    }
}

impl ToSql for u8 {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(i64::from(*self))
    }
}

impl ToSql for u32 {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        SqlValue::Integer(i64::from(*self))
    }
}

impl ToSql for Option<u32> {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        self.map_or(SqlValue::Null, |n| n.to_sql())
    }
}

impl ToSql for u64 {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        // Counts and object ids (at most 2^53 - 1) are far below i64::MAX.
        SqlValue::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
    }
}

impl ToSql for Option<u64> {
    const KIND: ColumnKind = ColumnKind::Integer;

    fn to_sql(&self) -> SqlValue {
        self.map_or(SqlValue::Null, |n| n.to_sql())
    }
}

row! {
    /// A release as built. Publishing sets its `published_at` once `latest` has moved to it, and
    /// pruning its `pruned_at`. A release whose `published_at` is NULL is not published, its
    /// rows possibly part loaded: readers list only releases with `published_at` set.
    ReleaseRow => "releases", scope: Releases, key: ["number"], indexes: [], {
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
        /// The estimated D1 bytes of the release's rows (CON-3), as its manifest gives it:
        /// publishing sums those of the releases not pruned against the cap.
        d1_estimated_bytes: u64,
    }
}

row! {
    /// The ids a release issued.
    IdsIssuedRow => "ids_issued", scope: Ledger, key: ["release"], indexes: [], {
        release: u32,
        first: u64,
        last: u64,
    }
}

row! {
    /// An id that left the releases, in `release`.
    IdReplacedRow => "ids_replaced", scope: Ledger, key: ["id"], indexes: [], {
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
    ObjectCellRow => "object_cells", scope: PerRelease,
    key: ["release", "domain", "ix", "iy", "iz", "object_id"], indexes: [], {
        release: u32,
        /// Term: its scale domain.
        domain: u32,
        ix: i64,
        iy: i64,
        iz: i64,
        object_id: u64,
    }
}

row! {
    /// A structure in one cell its bounding box meets.
    StructureCellRow => "structure_cells", scope: PerRelease,
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
    /// A term of the release: the name univ-model serializes for a value, which term columns
    /// hold by its id.
    TermRow => "terms", scope: PerRelease, key: ["release", "id"], indexes: [], {
        release: u32,
        id: u32,
        term: String,
    }
}

row! {
    /// A source of the release with its citation, terms and query.
    SourceRow => "sources", scope: PerRelease, key: ["release", "n"],
    indexes: [["release", "id"]], {
        release: u32,
        /// Its index in the release, which every `source` column of the release holds.
        n: u32,
        id: String,
        title: String,
        provider: String,
        scale_domain: String,
        citation: String,
        terms_url: String,
        terms_quote: String,
        credit: String,
        /// `permitted`, `permitted_with_credit` or `permission_pending`.
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
    PayloadRow => "payloads", scope: PerRelease, key: ["release", "source", "item"],
    indexes: [], {
        release: u32,
        source: u32,
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
    NormalizationRow => "normalizations", scope: PerRelease, key: ["release", "source"],
    indexes: [], {
        release: u32,
        source: u32,
        published: u64,
        /// JSON map of the entries left out by reason.
        left_out: String,
        fixes: String,
    }
}

row! {
    /// A researched structure: its record as its file gives it, every number cited.
    StructureRow => "structures", scope: PerRelease, key: ["release", "id"], indexes: [], {
        release: u32,
        id: String,
        name: String,
        kind: String,
        status: String,
        /// `stars`, `nearby_galaxies` or `cosmic_web`: the scale domain it is listed in.
        scale_domain: String,
        /// The centre of its geometry (its published position, a sphere's centre, the middle of
        /// a sky region, the centre of its placed members' box), ICRS Cartesian about the
        /// barycentre in its scale domain's unit (pc for `stars`, Mpc otherwise), under the
        /// release cosmology; none when it cannot be placed.
        centre_x: Option<f64>,
        centre_y: Option<f64>,
        centre_z: Option<f64>,
        geometry_kind: Option<String>,
        record: String,
        /// JSON of its member list as resolved at ingest: `shape`, each member an object at its
        /// derived position or a tracer, and `placed_from_file`, how each tracer was placed from
        /// the structure file's own sky position and redshift for it.
        members: Option<String>,
        /// JSON of its ICRS bounding box in Mpc, when it has one.
        bounds: Option<String>,
    }
}

row! {
    /// A scale domain's tiles.
    ScaleDomainRow => "scale_domains", scope: PerRelease, key: ["release", "name"], indexes: [], {
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
    GridRow => "grids", scope: PerRelease, key: ["release", "id"], indexes: [], {
        release: u32,
        id: String,
        source: u32,
        unit: String,
        shape: String,
        fields: String,
        header_key: String,
    }
}

row! {
    /// A source's primary reference and coverage, beside its row in `sources`.
    SourceDetailRow => "source_details", scope: PerRelease, key: ["release", "source"],
    indexes: [], {
        release: u32,
        /// The source's `sources.n`.
        source: u32,
        /// The primary reference's ADS bibcode, when it has one.
        bibcode: Option<String>,
        /// The primary reference's DOI without the `doi:` prefix, when it has one.
        doi: Option<String>,
        /// Why the source has no single primary reference, when it has none.
        reference_note: Option<String>,
        /// JSON `{"order": 5, "cells": [..]}`: the NESTED `HEALPix` cells, ICRS, ascending, that
        /// hold at least one of its published entries; none when no entry has a sky position.
        sky: Option<String>,
        /// JSON array of the radial ranges its entries span, one per measure; none when none.
        radial: Option<String>,
    }
}

row! {
    /// A structure's placed geometry and how it was derived, its outline test and its sizes
    /// rescaled to the release cosmology, beside its row in `structures`.
    StructureDetailRow => "structure_details", scope: PerRelease, key: ["release", "id"],
    indexes: [], {
        release: u32,
        /// The structure's `structures.id`.
        id: String,
        /// JSON of the univ-model shape of the placed structure, ICRS Cartesian about the
        /// barycentre in its scale domain's unit; none when it cannot be placed.
        shape: Option<String>,
        /// JSON of how `shape` and the structure's centre and bounds were derived; none when it
        /// is not placed.
        derivation: Option<String>,
        /// JSON of the result of the test that admitted or refused its outline on a volume
        /// grid; none for a structure not outlined from a grid.
        outline: Option<String>,
        /// JSON array of its lengths published under another cosmology, each as published and
        /// rescaled to the release cosmology; none when it has none.
        rescaled: Option<String>,
    }
}

row! {
    /// The conditions of a source's terms beyond its status, and the uncertainties it states
    /// once for its whole catalogue, beside its row in `sources` (schema version 4): written
    /// only for a source that has one of them.
    SourceTermRow => "source_terms", scope: PerRelease, key: ["release", "source"],
    indexes: [], {
        release: u32,
        /// The source's `sources.n`.
        source: u32,
        /// The clause of its terms that allows only non-commercial use, quoted; none when its
        /// terms allow commercial use.
        non_commercial: Option<String>,
        /// Whom permission to redistribute it was asked of, while its status is
        /// `permission_pending`.
        permission_asked_of: Option<String>,
        /// The date that permission was asked, YYYY-MM-DD.
        permission_asked_on: Option<String>,
        /// JSON array of the uncertainties it states once for its whole catalogue, each
        /// `{"quantity", "value", "unit", "times_one_plus_z", "basis"}`; none when it has none.
        catalogue_uncertainties: Option<String>,
    }
}

/// Every table a release writes rows to, in the order its D1 files load them.
pub const TABLES: [Table; 21] = [
    ReleaseRow::TABLE,
    TermRow::TABLE,
    SourceRow::TABLE,
    PayloadRow::TABLE,
    NormalizationRow::TABLE,
    ChainRow::TABLE,
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
    SourceDetailRow::TABLE,
    StructureDetailRow::TABLE,
    SourceTermRow::TABLE,
];

/// The key a common name is looked up by: lower case, without white space.
#[must_use]
pub fn name_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
