//! A source's provenance: every payload that arrived, with its digest, retrieval time, URL and
//! measure, kept as YAML beside its chunks and written before the payload is renamed into place.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;

use crate::fetch::{Payload, write_via_part};

/// The file name of a source's provenance, in the directory that holds its chunks.
pub const FILE: &str = "provenance.yaml";

/// What a source's provenance holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: Option<SourceId>,
    pub payloads: Vec<Payload>,
}

impl Provenance {
    /// The provenance in `dir`, or an empty one when there is none yet.
    ///
    /// # Errors
    ///
    /// Fails when the file exists but cannot be read or parsed.
    pub fn read(dir: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(dir.join(FILE)) {
            Ok(text) => serde_norway::from_str(&text).map_err(std::io::Error::other),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// The record of `item`, if there is one.
    #[must_use]
    pub fn payload(&self, item: &str) -> Option<&Payload> {
        self.payloads.iter().find(|p| p.item == item)
    }

    /// Records `payload` for `source` in `dir`, replacing an earlier record of the same item,
    /// and writes the file through a rename so that it is never half written.
    ///
    /// # Errors
    ///
    /// Fails when the provenance cannot be read or written.
    pub fn record(dir: &Path, source: &SourceId, payload: &Payload) -> std::io::Result<()> {
        let mut provenance = Self::read(dir)?;
        provenance.source = Some(source.clone());
        provenance.payloads.retain(|p| p.item != payload.item);
        provenance.payloads.push(payload.clone());
        let yaml = serde_norway::to_string(&provenance).map_err(std::io::Error::other)?;
        write_via_part(&dir.join(FILE), yaml.as_bytes(), || Ok(()))
    }
}
