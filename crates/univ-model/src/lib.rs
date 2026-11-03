//! The shared domain model of Univ Server and Univ Ingest: objects, measurements, distances,
//! frames, epochs, cosmologies, geometries, time records and layers.
//!
//! It compiles both for native targets and for `wasm32-unknown-unknown`, so it holds no I/O and
//! no threads.
