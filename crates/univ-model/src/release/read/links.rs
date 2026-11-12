//! An object's candidate links (`object_links`) and relations (`object_relations`), each a row
//! of its own, `n` its place among the object's (schema version 8).

use std::collections::BTreeMap;

use super::columns::Columns;
use super::entries::{confidence, named, reason};
use super::{ReadError, Row, Vocabulary};
use crate::object::{CandidateLink, Relation};

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
        Ok(CandidateLink {
            candidate: named(v, c, "candidate_source", "candidate_identifier")?,
            reason,
            confidence: confidence(c, "confidence")?,
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
