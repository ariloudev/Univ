//! An object's candidate links (`object_links`) and relations (`object_relations`), each a row
//! of its own, `n` its place among the object's (schema version 8).

use std::collections::BTreeMap;

use super::columns::Columns;
use super::entries::{designation, reason};
use super::{ReadError, Row, Vocabulary};
use crate::ids::Designation;
use crate::object::{CandidateLink, Confidence, Relation};

/// The designation of the source numbered in `source_column` and the identifier in
/// `identifier_column` of `c`.
fn named(
    v: &Vocabulary,
    c: &Columns<'_>,
    source_column: &str,
    identifier_column: &str,
) -> Result<Designation, ReadError> {
    let n = c.int(source_column)?;
    let source = v.source(n).ok_or_else(|| {
        c.error(
            source_column,
            format!("{n} is not among the release's sources"),
        )
    })?;
    designation(c, source, c.text(identifier_column)?)
}

/// The rows of `rows`, each read by `read`, in the order of their `n`.
fn in_order<T>(
    v: &Vocabulary,
    table: &'static str,
    rows: &[Row],
    read: impl Fn(&Columns<'_>) -> Result<T, ReadError>,
) -> Result<Vec<T>, ReadError> {
    let mut numbered = BTreeMap::new();
    for row in rows {
        let c = Columns::new(v, table, row);
        numbered.insert(c.small("n")?, read(&c)?);
    }
    Ok(numbered.into_values().collect())
}

/// The object's candidate links, from its `object_links` rows.
pub(super) fn links(v: &Vocabulary, rows: &[Row]) -> Result<Vec<CandidateLink>, ReadError> {
    in_order(v, "object_links", rows, |c| {
        let entry = named(v, c, "entry_source", "entry_identifier")?;
        let reason = reason(c, &entry, "reason_confidence", || {
            named(v, c, "via_source", "via_identifier")
        })?;
        let confidence = c.real("confidence")?;
        Ok(CandidateLink {
            candidate: named(v, c, "candidate_source", "candidate_identifier")?,
            reason,
            confidence: Confidence::new(confidence).map_err(|e| c.error("confidence", e))?,
            entry,
        })
    })
}

/// The object's relations, from its `object_relations` rows.
pub(super) fn relations(v: &Vocabulary, rows: &[Row]) -> Result<Vec<Relation>, ReadError> {
    in_order(v, "object_relations", rows, |c| {
        Ok(Relation {
            kind: c.typed("kind")?,
            entry: named(v, c, "entry_source", "entry_identifier")?,
            other: named(v, c, "other_source", "other_identifier")?,
            scheme: c.text("scheme")?.to_owned(),
            identifier: c.text("identifier")?.to_owned(),
        })
    })
}
