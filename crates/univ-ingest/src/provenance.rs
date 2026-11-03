//! A source's provenance: every payload that arrived, with its digest, retrieval time and URL,
//! kept as YAML beside its chunks and written before anything parses them.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;

use crate::fetch::Payload;

/// The file name of a source's provenance, in the directory that holds its chunks.
pub const FILE: &str = "provenance.yaml";

/// What a source's provenance holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: Option<SourceId>,
    pub payloads: Vec<PayloadRecord>,
}

/// One payload as it arrived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadRecord {
    pub item: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub retrieved: String,
}

impl From<&Payload> for PayloadRecord {
    fn from(p: &Payload) -> Self {
        Self {
            item: p.item.clone(),
            url: p.url.clone(),
            sha256: p.sha256.clone(),
            bytes: p.bytes,
            retrieved: p.retrieved.clone(),
        }
    }
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
        provenance.payloads.push(payload.into());
        let yaml = serde_norway::to_string(&provenance).map_err(std::io::Error::other)?;
        let part = dir.join(format!("{FILE}.part"));
        std::fs::write(&part, yaml)?;
        std::fs::rename(part, dir.join(FILE))
    }
}
