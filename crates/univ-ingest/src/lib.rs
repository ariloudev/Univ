//! Univ Ingest: downloads public sky data, normalizes it, resolves catalogue entries into objects
//! and publishes the result as a release that Univ Server serves.

pub mod access;
pub mod acquire;
pub mod display;
pub mod events;
pub mod fetch;
pub mod normalize;
pub mod provenance;
pub mod run;
pub mod source;
pub mod table;
