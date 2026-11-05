//! A source's provenance: every payload that arrived, with its digest, retrieval time, URL and
//! measure, kept as YAML beside its chunks and written before the payload is renamed into place,
//! and what normalization last made of them.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;

use crate::fetch::Payload;
use crate::normalize::NormalizationReport;

/// The file name of a source's provenance, in the directory that holds its chunks.
pub const FILE: &str = "provenance.yaml";

/// What a source's provenance holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: Option<SourceId>,
    pub payloads: Vec<Payload>,
    /// Entries published and left out by reason, and fixes applied, when the payloads were
    /// last normalized; none once a payload has changed since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalization: Option<NormalizationReport>,
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

    /// Records `payload` for `source`, replacing an earlier record of the same item. The
    /// normalization recorded before no longer describes the payloads, so it is dropped.
    pub fn recorded(&mut self, source: &SourceId, payload: &Payload) {
        self.source = Some(source.clone());
        self.payloads.retain(|p| p.item != payload.item);
        self.payloads.push(payload.clone());
        // Chunks arrive in the order their fetches finish; keep the record in item order, with
        // numbers compared as numbers, so the same payloads always give the same file.
        self.payloads.sort_by_cached_key(|p| natural_key(&p.item));
        self.normalization = None;
    }

    /// Writes the provenance into `dir` through a rename, so that it is never half written.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        crate::yaml::write(&dir.join(FILE), self)
    }

    /// Records `payload` for `source` in the provenance kept in `dir`.
    ///
    /// # Errors
    ///
    /// Fails when the provenance cannot be read or written.
    pub fn record(dir: &Path, source: &SourceId, payload: &Payload) -> std::io::Result<()> {
        let mut provenance = Self::read(dir)?;
        provenance.recorded(source, payload);
        provenance.write(dir)
    }

    /// Records what normalization made of `source`'s payloads in the provenance kept in `dir`.
    ///
    /// # Errors
    ///
    /// Fails when the provenance cannot be read or written.
    pub fn normalized(
        dir: &Path,
        source: &SourceId,
        report: &NormalizationReport,
    ) -> std::io::Result<()> {
        let mut provenance = Self::read(dir)?;
        provenance.source = Some(source.clone());
        provenance.normalization = Some(report.clone());
        provenance.write(dir)
    }

    /// Removes the normalization report from the provenance kept in `dir`, after a
    /// normalization that failed or was cancelled: an earlier report no longer describes the
    /// source. A provenance that holds none is left as it is.
    ///
    /// # Errors
    ///
    /// Fails when the provenance cannot be read or written.
    pub fn unnormalized(dir: &Path) -> std::io::Result<()> {
        let mut provenance = Self::read(dir)?;
        if provenance.normalization.take().is_some() {
            provenance.write(dir)?;
        }
        Ok(())
    }
}

/// A key that orders item names with their runs of digits compared as numbers: `chunk 2 of 17`
/// before `chunk 10 of 17`.
fn natural_key(item: &str) -> Vec<(String, u64)> {
    let mut key = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    for c in item.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                key.push((
                    std::mem::take(&mut text),
                    digits.parse().unwrap_or(u64::MAX),
                ));
                digits.clear();
            }
            text.push(c);
        }
    }
    let last = if digits.is_empty() {
        0
    } else {
        digits.parse().unwrap_or(u64::MAX)
    };
    key.push((text, last));
    key
}
