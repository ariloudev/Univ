//! Reading an object back from a release's compact D1 rows, with no I/O: the caller queries
//! the rows and hands them over as JSON maps, as D1 returns them, with the release's
//! [`Vocabulary`] read once.
//!
//! Every code is decoded through the vocabulary (terms, sources and chain templates) and every
//! uncertainty through [`crate::release::UncertaintyColumns`], so nothing here depends on how a
//! release numbered its terms. Rows that do not decode are the server's fault, never the
//! client's: a [`ReadError`] names the table, the column and the value.

mod columns;
mod entries;
mod links;
mod measurements;
mod placed;

use std::collections::{BTreeMap, HashMap};

use crate::api::objects::ObjectRecord;
use crate::cosmology::Cosmology;
use columns::Columns;

/// One D1 row: column names to numbers, strings and nulls.
pub type Row = serde_json::Map<String, serde_json::Value>;

/// A release's vocabulary: everything the rows' codes need, read once per release.
#[derive(Clone, Debug)]
pub struct Vocabulary {
    release: u32,
    cosmology: Cosmology,
    ranking_rule: String,
    terms: HashMap<i64, String>,
    chains: HashMap<i64, serde_json::Value>,
    sources: HashMap<i64, String>,
    term_ids: HashMap<String, i64>,
    source_ns: HashMap<String, i64>,
    /// The basis of each uncertainty a source states once for its whole catalogue, by the
    /// source's id and the quantity it applies to, such as `redshift`.
    catalogue_uncertainties: HashMap<(String, String), String>,
}

impl Vocabulary {
    /// The vocabulary of release `release`, placed under `cosmology` and ranked by the rule
    /// named `ranking_rule`: its `terms` by id, its chain templates by id (`chains.steps`
    /// parsed), and its sources' ids by `sources.n`.
    #[must_use]
    pub fn new(
        release: u32,
        cosmology: Cosmology,
        ranking_rule: String,
        terms: impl IntoIterator<Item = (i64, String)>,
        chains: impl IntoIterator<Item = (i64, serde_json::Value)>,
        sources: impl IntoIterator<Item = (i64, String)>,
    ) -> Self {
        let terms: HashMap<i64, String> = terms.into_iter().collect();
        let sources: HashMap<i64, String> = sources.into_iter().collect();
        Self {
            release,
            cosmology,
            ranking_rule,
            term_ids: terms.iter().map(|(id, t)| (t.clone(), *id)).collect(),
            source_ns: sources.iter().map(|(n, id)| (id.clone(), *n)).collect(),
            terms,
            chains: chains.into_iter().collect(),
            sources,
            catalogue_uncertainties: HashMap::new(),
        }
    }

    /// The vocabulary with the uncertainties its sources state once for their whole
    /// catalogues, each as the source's id, the quantity and where the source states it.
    #[must_use]
    pub fn with_catalogue_uncertainties(
        mut self,
        uncertainties: impl IntoIterator<Item = (String, String, String)>,
    ) -> Self {
        self.catalogue_uncertainties = uncertainties
            .into_iter()
            .map(|(source, quantity, basis)| ((source, quantity), basis))
            .collect();
        self
    }

    /// Where source `source` states the one uncertainty of its whole catalogue for `quantity`,
    /// when it states one.
    #[must_use]
    pub fn catalogue_uncertainty(&self, source: &str, quantity: &str) -> Option<&str> {
        self.catalogue_uncertainties
            .get(&(source.to_owned(), quantity.to_owned()))
            .map(String::as_str)
    }

    #[must_use]
    pub fn release(&self) -> u32 {
        self.release
    }

    /// The name of the release's ranking rule, which chose each preferred measurement.
    #[must_use]
    pub fn ranking_rule(&self) -> &str {
        &self.ranking_rule
    }

    /// The release cosmology, which every derived position of the release is placed under.
    #[must_use]
    pub fn cosmology(&self) -> &Cosmology {
        &self.cosmology
    }

    /// The id of term `term`.
    #[must_use]
    pub fn term_id(&self, term: &str) -> Option<i64> {
        self.term_ids.get(term).copied()
    }

    /// The `n` of the source of id `id`.
    #[must_use]
    pub fn source_n(&self, id: &str) -> Option<i64> {
        self.source_ns.get(id).copied()
    }

    /// The text of term `id`.
    #[must_use]
    pub fn term(&self, id: i64) -> Option<&str> {
        self.terms.get(&id).map(String::as_str)
    }

    /// The id of the source numbered `n`.
    #[must_use]
    pub fn source(&self, n: i64) -> Option<&str> {
        self.sources.get(&n).map(String::as_str)
    }

    /// The template of chain `id`.
    #[must_use]
    pub fn chain(&self, id: i64) -> Option<&serde_json::Value> {
        self.chains.get(&id)
    }
}

/// Why rows did not decode: always the server's fault, naming the table, column and value.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadError(pub String);

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReadError {}

/// The object of row `object` with its `entries`, `measurements` and `designations` rows, each
/// every row of the object in the release, in any order. A `designations` row holds `source`,
/// its source's id, and `identifier`, as a join of `designations` with `sources` gives them.
///
/// The record's designations are those rows. A release built before the `designations` table
/// holds none for any object: given no rows, the record gives the entries' own designations.
///
/// # Errors
///
/// Fails when a row lacks a column, a column holds a value of the wrong type, a code names no
/// term, source or chain of the release, or the values do not make a valid record.
pub fn object(
    v: &Vocabulary,
    object: &Row,
    entries: &[Row],
    measurements: &[Row],
    designations: &[Row],
) -> Result<ObjectRecord, ReadError> {
    let c = Columns::new(v, "objects", object);
    let mut heads = BTreeMap::new();
    let mut sorted_entries = Vec::with_capacity(entries.len());
    for row in entries {
        let (n, designation) = entries::head(v, row)?;
        if heads.insert(n, designation).is_some() {
            let c = Columns::new(v, "entries", row);
            return Err(c.error("entry", format!("{n} appears twice")));
        }
        sorted_entries.push((n, row));
    }
    sorted_entries.sort_by_key(|(n, _)| *n);
    let entries = entries::entries(v, &sorted_entries, &heads)?;

    let mut keyed = measurements
        .iter()
        .map(|row| Ok((measurements::key(v, row)?, row)))
        .collect::<Result<Vec<_>, ReadError>>()?;
    keyed.sort_by_key(|(k, _)| *k);
    let measurements = keyed
        .into_iter()
        .map(|(key, row)| measurements::measurement(v, row, key, &heads))
        .collect::<Result<Vec<_>, _>>()?;

    let preferred = placed::preferred(&c, v.ranking_rule())?;
    let position = if c.int("placed")? == 0 {
        None
    } else {
        Some(placed::derived(
            &c,
            preferred.as_ref(),
            &measurements,
            &entries,
        )?)
    };
    let id = c.int("id")?;
    // The `designations` rows, which add those sources give beside their entries; the entries'
    // own for a release from before that table.
    let mut designations = if designations.is_empty() {
        heads.into_values().collect()
    } else {
        entries::designations(v, designations)?
    };
    designations.sort();
    designations.dedup();
    Ok(ObjectRecord {
        designations,
        release: v.release(),
        id: u64::try_from(id).map_err(|_| c.error("id", format!("{id} is negative")))?,
        kind: c.typed("kind")?,
        sky_position: placed::sky(&c)?,
        preferred,
        position,
        not_placed: c.typed_opt("not_placed")?,
        links: links::links(&c)?,
        relations: c.json_opt("relations")?.unwrap_or_default(),
        entries,
        measurements,
    })
}

#[cfg(test)]
mod tests;
