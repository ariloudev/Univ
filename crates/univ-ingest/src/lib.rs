//! Univ Ingest: downloads public sky data, normalizes it, resolves catalogue entries into objects
//! and publishes the result as a release that Univ Server serves.

pub mod access;
pub mod acquire;
pub mod crossmatch;
pub mod display;
pub mod events;
pub mod fetch;
pub mod fits;
pub mod grids;
pub mod hash;
pub mod ids;
pub mod lock;
pub mod matching;
pub mod normalize;
pub mod outlines;
pub mod provenance;
pub mod publish;
pub mod ranking;
pub mod release;
pub mod run;
pub mod source;
pub mod stars;
pub mod state;
pub mod structures;
pub mod table;
pub mod tiles;
pub mod watch;
pub mod yaml;
