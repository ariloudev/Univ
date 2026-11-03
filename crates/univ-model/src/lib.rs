//! The shared domain model of Univ Server and Univ Ingest: objects, measurements, distances,
//! frames, epochs, cosmologies, geometries, volume grids, time records and layers.
//!
//! It compiles both for native targets and for `wasm32-unknown-unknown`, so it holds no I/O and
//! no threads.

pub mod cosmology;
pub mod distance;
pub mod geometry;
pub mod grid;
pub mod ids;
pub mod position;
pub mod redshift;
pub mod units;
pub mod value;
