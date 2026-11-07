//! The layout of a release as Univ Server reads it: its D1 schema with a Rust type per row, the
//! R2 keys of its files, the cell index of bounded volume queries, and the release manifest.
//!
//! # A release column in each database
//!
//! Every release lives in the same D1 databases (since version 5 a catalogue database and object
//! shards, [`shards`]). Each table that holds one release's rows has a
//! `release` column leading its primary key, rather than a table per release:
//!
//! - **Publishing never disturbs the current release.** Release n + 1's rows are inserted
//!   beside release n's, under their own release number, which no read of release n selects;
//!   reads that name no release follow the single row of `latest`, which moves in one write once
//!   the new rows are all in.
//! - **Pruning drops one release.** It deletes the rows whose release is n from the tables
//!   whose [`Table::scope`] is [`Scope::PerRelease`] and sets the release's `pruned_at`; the list of releases and the id
//!   ledger (`ids_issued`, `ids_replaced`) are kept, so every id still resolves.
//! - **One schema.** Queries name fixed tables, and the schema changes once, under
//!   [`SCHEMA_VERSION`], not per release.
//!
//! - **Readers list only published releases.** A release's row in `releases` is loaded with its
//!   other rows, before `latest` moves to it; its `published_at` is set only once `latest` has.
//!   A release whose `published_at` is NULL may be part loaded, and readers never list it.
//!
//! The CON-3 cap of 10 GB applies to the database: [`sql::Table::estimated_bytes`] gives each
//! row's estimate, so a build checks one release's rows against [`RELEASE_BUDGET_BYTES`], under
//! half the cap, before anything is published, and publishing checks the sum of the estimates of
//! every release not pruned and the new one, each recorded in its `releases.d1_estimated_bytes`.
//!
//! # Compact rows (schema version 2)
//!
//! Repeated names are numbered per release in `terms`, sources by `sources.n`, an object's
//! entries by their place among its entries, an uncertainty is held in four typed columns
//! ([`uncertainty`]), a normalized value only where it is not the native one, and each distinct
//! transformation chain once in `chains` ([`chains`]), so that a release of millions of objects
//! keeps every row within its B-tree cell.
//!
//! # Details (schema version 3)
//!
//! Version 3 adds two tables, changing none of version 2's: `source_details`, each source's
//! primary reference and the sky and radial coverage of its entries, and `structure_details`,
//! each structure's placed geometry with how it was derived, the test of its outline on a volume
//! grid, and its sizes published under another cosmology beside their rescaled values.
//!
//! # Source terms (schema version 4)
//!
//! Version 4 adds one table, changing none of version 3's: `source_terms`, the clause that
//! limits a source to non-commercial use, the request for permission of a source whose status
//! is `permission_pending`, and the uncertainties a source states once for its whole catalogue,
//! which the object read gives each such measurement as its estimated basis.
//!
//! # Object shards (schema version 5)
//!
//! Version 5 divides a release among a catalogue database and object shards ([`shards`]), each
//! given the whole schema, and adds three catalogue tables, changing none of version 4's:
//! `object_shards`, the shard of each object id; `shard_cells`, the run of cells each shard holds
//! in each scale domain; and `release_shards`, each shard's objects and estimate. CON-3's cap
//! applies to each database, and so does the budget of one release.

pub mod cells;
pub mod chains;
pub mod fingerprint;
pub mod keys;
pub mod manifest;
pub mod read;
pub mod rows;
pub mod shards;
pub mod shared;
pub mod sql;
pub mod statements;
pub mod uncertainty;

/// The real local state, relative to the workspace: the local D1 and R2 that `univ-ingest
/// publish` puts releases in, and that `cargo xtask serve`, `explore` and the benches run Univ
/// Server on (`--persist-to` of `wrangler dev`). One constant for both, so that a publish never
/// lands where nothing reads it, as release 10's once did in Wrangler's own `.wrangler/state`.
pub const REAL_LOCAL_STATE: &str = ".cache/univ-ingest/wrangler";

/// Where Wrangler keeps the local D1's `SQLite` files, within its state directory: one file a
/// database, each named by a hash of its id, so what each holds tells which it is.
pub const LOCAL_D1_FILES: &str = "v3/d1/miniflare-D1DatabaseObject";

pub use cells::{Cell, CellRange, CellSpace};
pub use fingerprint::{ColumnTotal, Fingerprinter};
pub use manifest::{D1_CAP_BYTES, RELEASE_BUDGET_BYTES, ReleaseManifest};
pub use rows::TABLES;
pub use shared::{RETIRED, valid_in};
pub use sql::{ColumnKind, Row, Scope, SqlValue, Table, Tree};
pub use statements::Statements;
pub use uncertainty::UncertaintyColumns;

/// The version of the D1 schema in [`SCHEMA`].
pub const SCHEMA_VERSION: u32 = 6;

/// The D1 schema: every table and index, created only where missing.
pub const SCHEMA: &str = include_str!("schema.sql");

#[cfg(test)]
mod tests;
