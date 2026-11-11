//! The rows that hold, since schema version 8, what tables per object and per catalogue entry
//! once held as JSON (keep-json-out-of-the-tables-that-grow-with-a-releases-objects): an entry's
//! flags and references, a measurement's details, an object's candidate links and relations,
//! and the ids that replaced an id. A build writes them from its objects; `univ-ingest migrate`
//! writes them from the JSON of a release of schema 7, through the same functions.

use serde::Serialize;
use univ_model::cosmology::AssumedCosmology;
use univ_model::entry::PaperReference;
use univ_model::ids::{Designation, SourceId};
use univ_model::object::{CandidateLink, LineOfSight, Relation, Resolution};
use univ_model::release::UncertaintyColumns;
use univ_model::release::rows::{
    EntryFlagRow, EntryReferenceRow, IdReplacementRow, MeasurementDetailRow, MembershipRow,
    ObjectLinkRow, ObjectRelationRow,
};
use univ_model::value::Quantity;

use super::entries::{Numbering, small};
use crate::source::Membership;
use univ_model::object::ObjectEntry;

/// What numbers the codes of a release's rows: its terms by name, and its sources by id.
pub trait Codes {
    /// The release the rows are written in.
    fn release(&self) -> u32;
    /// The id of the term `name`.
    fn term(&mut self, name: &str) -> u32;
    /// The id of the term `value` serializes as.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be written as a term.
    fn term_of<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<u32, String>;
    /// The number of the source `source`.
    fn source(&mut self, source: &SourceId) -> u32;
}

impl Codes for Numbering {
    fn release(&self) -> u32 {
        self.release
    }

    fn term(&mut self, name: &str) -> u32 {
        self.terms.id(name)
    }

    fn term_of<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<u32, String> {
        self.terms.of(value)
    }

    fn source(&mut self, source: &SourceId) -> u32 {
        self.sources.n(source)
    }
}

/// A resolution as the columns of an entry's or a candidate link's reason hold it, and the
/// designation it was made through, which the caller places.
pub struct Reason<'r> {
    pub resolution: u32,
    pub scheme: Option<u32>,
    pub shared: Option<String>,
    pub via: Option<&'r Designation>,
    pub separation_arcsec: Option<f64>,
    pub difference: Option<u32>,
    pub delta: Option<f64>,
    pub confidence: Option<f64>,
}

/// The columns of `r`, a resolution of the entry `own`.
///
/// # Errors
///
/// Fails when a match method cannot be written as a term.
pub fn reason<'r>(
    codes: &mut impl Codes,
    own: &Designation,
    r: &'r Resolution,
) -> Result<Reason<'r>, String> {
    let mut reason = Reason {
        resolution: codes.term("anchor"),
        scheme: None,
        shared: None,
        via: None,
        separation_arcsec: None,
        difference: None,
        delta: None,
        confidence: None,
    };
    match r {
        Resolution::Anchor => {}
        Resolution::CrossIdentification {
            scheme,
            identifier,
            via,
            confidence,
        } => {
            reason.resolution = codes.term("cross_identification");
            reason.scheme = Some(codes.term(scheme));
            reason.shared = (**identifier != *own.identifier).then(|| identifier.clone());
            reason.via = Some(via);
            reason.confidence = Some(confidence.get());
        }
        Resolution::Match(m) => {
            reason.resolution = codes.term("match");
            reason.scheme = Some(codes.term_of(&m.method)?);
            reason.via = Some(&m.with);
            reason.separation_arcsec = Some(m.separation_arcsec);
            (reason.difference, reason.delta) = match m.difference {
                None => (None, None),
                Some(LineOfSight::Velocity { delta_km_s }) => {
                    (Some(codes.term("velocity")), Some(delta_km_s))
                }
                Some(LineOfSight::Distance { delta_fraction }) => {
                    (Some(codes.term("distance")), Some(delta_fraction))
                }
            };
            reason.confidence = Some(m.confidence.get());
        }
    }
    Ok(reason)
}

/// The rows of the flags of entry `entry` of object `object_id`, each a column name, coded as
/// a term, and its value as published.
pub fn flag_rows<'f>(
    codes: &mut impl Codes,
    object_id: u64,
    entry: u32,
    flags: impl IntoIterator<Item = (&'f str, &'f str)>,
) -> Vec<EntryFlagRow> {
    flags
        .into_iter()
        .map(|(flag, value)| EntryFlagRow {
            release: codes.release(),
            object_id,
            entry,
            flag: codes.term(flag),
            value: value.to_owned(),
        })
        .collect()
}

/// The rows of the papers of entry `entry` of object `object_id`, in their order.
#[must_use]
pub fn reference_rows(
    release: u32,
    object_id: u64,
    entry: u32,
    papers: &[PaperReference],
) -> Vec<EntryReferenceRow> {
    papers
        .iter()
        .enumerate()
        .map(|(n, p)| EntryReferenceRow {
            release,
            object_id,
            entry,
            n: small(n),
            citation: p.citation.clone(),
            doi: p.doi.clone(),
            bibcode: p.bibcode.clone(),
        })
        .collect()
}

/// The row of what measurement `n` of `quantity` (a term) of entry `entry` holds beside its
/// values: a distance's value as also published and its assumed cosmology, a colour index's
/// two magnitude columns; none when it holds none of them.
#[expect(
    clippy::too_many_arguments,
    reason = "the row's key, each column named where it is written"
)]
pub fn detail_row(
    codes: &mut impl Codes,
    object_id: u64,
    entry: u32,
    quantity: u32,
    n: u32,
    also: Option<&Quantity>,
    assumed: Option<&AssumedCosmology>,
    derived_from: Option<&[String; 2]>,
) -> Result<Option<MeasurementDetailRow>, String> {
    if also.is_none() && assumed.is_none() && derived_from.is_none() {
        return Ok(None);
    }
    let u = also.map(|q| UncertaintyColumns::from(q.uncertainty));
    Ok(Some(MeasurementDetailRow {
        release: codes.release(),
        object_id,
        entry,
        quantity,
        n,
        also_value: also.map(|q| q.value),
        also_unit: also.map(|q| codes.term_of(&q.unit)).transpose()?,
        also_form: u.map(|u| codes.term(u.form)),
        also_minus: u.and_then(|u| u.minus),
        also_plus: u.and_then(|u| u.plus),
        also_level: u.and_then(|u| u.level),
        assumed_hubble_constant: assumed.map(|c| c.hubble_constant),
        assumed_omega_matter: assumed.and_then(|c| c.omega_matter),
        assumed_omega_lambda: assumed.and_then(|c| c.omega_lambda),
        derived_from_first: derived_from.map(|[a, _]| a.clone()),
        derived_from_second: derived_from.map(|[_, b]| b.clone()),
    }))
}

/// The rows of object `object_id`'s candidate links, in their order.
///
/// # Errors
///
/// Fails when a reason's match method cannot be written as a term.
pub fn link_rows(
    codes: &mut impl Codes,
    object_id: u64,
    links: &[CandidateLink],
) -> Result<Vec<ObjectLinkRow>, String> {
    links
        .iter()
        .enumerate()
        .map(|(n, link)| {
            let r = reason(codes, &link.entry, &link.reason)?;
            Ok(ObjectLinkRow {
                release: codes.release(),
                object_id,
                n: small(n),
                entry_source: codes.source(&link.entry.source),
                entry_identifier: String::from(&*link.entry.identifier),
                candidate_source: codes.source(&link.candidate.source),
                candidate_identifier: String::from(&*link.candidate.identifier),
                resolution: r.resolution,
                scheme: r.scheme,
                shared: r.shared,
                via_source: r.via.map(|d| codes.source(&d.source)),
                via_identifier: r.via.map(|d| String::from(&*d.identifier)),
                separation_arcsec: r.separation_arcsec,
                difference: r.difference,
                delta: r.delta,
                reason_confidence: r.confidence,
                confidence: link.confidence.get(),
            })
        })
        .collect()
}

/// The rows of object `object_id`'s relations, in their order.
///
/// # Errors
///
/// Fails when a relation's kind cannot be written as a term.
pub fn relation_rows(
    codes: &mut impl Codes,
    object_id: u64,
    relations: &[Relation],
) -> Result<Vec<ObjectRelationRow>, String> {
    relations
        .iter()
        .enumerate()
        .map(|(n, r)| {
            Ok(ObjectRelationRow {
                release: codes.release(),
                object_id,
                n: small(n),
                kind: codes.term_of(&r.kind)?,
                entry_source: codes.source(&r.entry.source),
                entry_identifier: String::from(&*r.entry.identifier),
                other_source: codes.source(&r.other.source),
                other_identifier: String::from(&*r.other.identifier),
                scheme: r.scheme.clone(),
                identifier: r.identifier.clone(),
            })
        })
        .collect()
}

/// The rows of object `object_id`'s memberships: each structure that a flag value of one of its
/// entries names, through `memberships` of the entry's source. A structure two entries name is
/// one membership, the first entry's.
pub fn membership_rows<'m>(
    codes: &mut impl Codes,
    object_id: u64,
    entries: &[ObjectEntry],
    memberships: impl Fn(&SourceId) -> &'m [Membership],
) -> Vec<MembershipRow> {
    let mut rows: Vec<MembershipRow> = Vec::new();
    for e in entries {
        let source = &e.entry.designation.source;
        for m in memberships(source) {
            let Some(value) = e.entry.flags.get(&m.column) else {
                continue;
            };
            let Some(structure) = m.structure(value) else {
                continue;
            };
            if rows.iter().any(|r| r.structure == structure) {
                continue;
            }
            rows.push(MembershipRow {
                release: codes.release(),
                object_id,
                structure: structure.to_owned(),
                source: codes.source(source),
                value: value.to_owned(),
            });
        }
    }
    rows
}

/// The rows of the ids that replaced id `id` in `release`, ascending.
#[must_use]
pub fn replacement_rows(id: u64, release: u32, replaced_by: &[u64]) -> Vec<IdReplacementRow> {
    replaced_by
        .iter()
        .enumerate()
        .map(|(n, &by)| IdReplacementRow {
            id,
            n: small(n),
            release,
            replaced_by: by,
        })
        .collect()
}
