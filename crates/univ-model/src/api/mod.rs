//! What Univ Server answers with: the documents of its API, as Rust types, so that the OpenAPI
//! document's schemas are generated from them (read-the-openapi-document) and a client in
//! another language can be generated in turn.
//!
//! Times are RFC 3339 UTC strings, as the release database records them.

pub mod bulk;
pub mod keys;
pub mod layers;
pub mod lookup;
pub mod objects;
pub mod query;
pub mod releases;
pub mod sources;
pub mod structures;
