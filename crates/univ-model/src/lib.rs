//! The shared domain model of Univ Server and Univ Ingest: objects, measurements, distances,
//! frames, epochs, cosmologies, geometries, volume grids, tiles, time records and layers.
//!
//! It compiles both for native targets and for `wasm32-unknown-unknown`, so it holds no I/O and
//! no threads.

pub mod api;
pub mod cosmology;
pub mod derive;
pub mod distance;
pub mod encoding;
pub mod entry;
pub mod geometry;
pub mod grid;
pub mod ids;
pub mod ledger;
pub mod object;
pub mod position;
pub mod redshift;
pub mod release;
pub mod tile;
pub mod time;
pub mod units;
pub mod value;
