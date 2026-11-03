//! The layout of a release as Univ Server reads it: its D1 schema with a Rust type per row, the
//! R2 keys of its files, the cell index of bounded volume queries, and the release manifest.
//!
//! # One database, a release column
//!
//! Every release lives in one D1 database. Each table that holds one release's rows has a
//! `release` column leading its primary key, rather than a table per release:
//!
//! - **Publishing never disturbs the current release.** Release n + 1's rows are inserted
//!   beside release n's, under their own release number, which no read of release n selects;
//!   reads that name no release follow the single row of `latest`, which moves in one write once
//!   the new rows are all in.
//! - **Pruning drops one release.** It deletes the rows whose release is n from the tables of
//!   [`Table::per_release`] and sets the release's `pruned_at`; the list of releases and the id
//!   ledger (`ids_issued`, `ids_replaced`) are kept, so every id still resolves.
//! - **One schema.** Queries name fixed tables, and the schema changes once, under
//!   [`SCHEMA_VERSION`], not per release.
//!
//! The CON-3 cap of 10 GB applies to one release's rows: [`sql::Table::estimated_bytes`] gives
//! each row's estimate, so a build can check it before anything is published. A database
//! holding several releases holds their sum, which publishing checks against D1's own limit.

pub mod cells;
pub mod keys;
pub mod manifest;
pub mod rows;
pub mod sql;

pub use cells::{Cell, CellRange, CellSpace};
pub use manifest::{D1_CAP_BYTES, ReleaseManifest};
pub use rows::TABLES;
pub use sql::{Row, SqlValue, Table};

/// The version of the D1 schema in [`SCHEMA`].
pub const SCHEMA_VERSION: u32 = 1;

/// The D1 schema: every table and index, created only where missing.
pub const SCHEMA: &str = include_str!("schema.sql");

#[cfg(test)]
mod tests;
