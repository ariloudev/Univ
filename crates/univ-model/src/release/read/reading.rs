//! How a release's sources are read (schema version 7): the form each source's catalogue writes
//! a designation in and the rank the designation rules give it, which order and format an
//! object's designations and names.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::columns::Columns;
use super::{ReadError, Row, Vocabulary};
use crate::api::objects::{DesignationRecord, NameRecord};
use crate::ids::Designation;

/// The placeholder a designation form holds once, where the identifier goes.
pub const IDENTIFIER: &str = "{identifier}";

/// Each source's designation form and rank, by the source's id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    forms: HashMap<String, String>,
    ranks: HashMap<String, u32>,
}

impl Reading {
    /// The reading of sources given as their id, designation form and rank.
    pub fn new(sources: impl IntoIterator<Item = (String, Option<String>, Option<u32>)>) -> Self {
        let mut reading = Self::default();
        for (source, form, rank) in sources {
            if let Some(form) = form {
                reading.forms.insert(source.clone(), form);
            }
            if let Some(rank) = rank {
                reading.ranks.insert(source, rank);
            }
        }
        reading
    }

    /// `identifier` of `source` in its catalogue's form, when the source has one.
    #[must_use]
    pub fn designation(&self, source: &str, identifier: &str) -> Option<String> {
        let form = self.forms.get(source)?;
        Some(form.replacen(IDENTIFIER, identifier, 1))
    }

    /// The identifier `text` is in the form of each source, as the source and the identifier:
    /// `HIP 32754` is Hipparcos's `32754`. Letter case is ignored around the identifier.
    #[must_use]
    pub fn parse(&self, text: &str) -> Vec<(String, String)> {
        let text = text.trim();
        let mut found: Vec<(String, String)> = self
            .forms
            .iter()
            .filter_map(|(source, form)| {
                let (before, after) = form.split_once(IDENTIFIER)?;
                let lower = text.to_lowercase();
                let rest = lower.strip_prefix(&before.to_lowercase())?;
                let rest = rest.strip_suffix(&after.to_lowercase())?;
                let start = before.len();
                let identifier = text.get(start..start + rest.len())?.trim();
                (!identifier.is_empty()).then(|| (source.clone(), identifier.to_owned()))
            })
            .collect();
        found.sort();
        found
    }

    /// How two sources are ordered: the ranked first, by rank, then by id.
    fn order(&self, a: &str, b: &str) -> Ordering {
        let rank = |s: &str| self.ranks.get(s).copied().unwrap_or(u32::MAX);
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    }

    /// `designations` as records in the order of their sources' ranks, then by source and
    /// identifier, once each. Those among `own`, entries' own identifiers, are given in their
    /// catalogue's form; one a source gives beside an entry from another field, such as the HIP
    /// number of a Gaia–Hipparcos pair, is another catalogue's and is given as it is kept.
    #[must_use]
    pub fn designations(
        &self,
        designations: Vec<Designation>,
        own: &BTreeSet<Designation>,
    ) -> Vec<DesignationRecord> {
        let mut records: Vec<DesignationRecord> = designations
            .into_iter()
            .map(|d| DesignationRecord {
                designation: own
                    .contains(&d)
                    .then(|| self.designation(d.source.as_str(), &d.identifier))
                    .flatten(),
                source: d.source.as_str().to_owned(),
                identifier: String::from(&*d.identifier),
            })
            .collect();
        records.sort_by(|a, b| {
            self.order(&a.source, &b.source)
                .then_with(|| a.identifier.cmp(&b.identifier))
        });
        records.dedup();
        records
    }
}

/// An object's `names` rows, each its source by number (`source`) and its `name`, in the order
/// of their sources' ranks, then by source and name, once each.
///
/// # Errors
///
/// Fails when a row lacks a column or names no source of the release.
pub fn names(v: &Vocabulary, rows: &[Row]) -> Result<Vec<NameRecord>, ReadError> {
    let mut names = rows
        .iter()
        .map(|row| {
            let c = Columns::new(v, "names", row);
            let n = c.int("source")?;
            let source = v.source(n).ok_or_else(|| {
                c.error("source", format!("{n} is not among the release's sources"))
            })?;
            Ok(NameRecord {
                name: c.text("name")?.to_owned(),
                source: source.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, ReadError>>()?;
    let reading = v.reading();
    names.sort_by(|a, b| {
        reading
            .order(&a.source, &b.source)
            .then_with(|| a.name.cmp(&b.name))
    });
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SourceId;

    fn reading() -> Reading {
        Reading::new([
            (
                "hipparcos-2007".to_owned(),
                Some("HIP {identifier}".to_owned()),
                Some(1),
            ),
            (
                "twomrs".to_owned(),
                Some("2MASX J{identifier}".to_owned()),
                Some(3),
            ),
            (
                "gaia-dr3-bright".to_owned(),
                Some("Gaia DR3 {identifier}".to_owned()),
                Some(2),
            ),
            ("mcconnachie-2012".to_owned(), None, None),
        ])
    }

    fn designation(source: &str, identifier: &str) -> Designation {
        Designation::new(SourceId::new(source).unwrap(), identifier.to_owned()).unwrap()
    }

    #[test]
    fn an_identifier_is_written_in_its_sources_form_and_left_bare_without_one() {
        let r = reading();
        assert_eq!(
            r.designation("hipparcos-2007", "32754").as_deref(),
            Some("HIP 32754")
        );
        assert_eq!(r.designation("mcconnachie-2012", "LMC"), None);
    }

    #[test]
    fn a_designation_in_a_catalogues_form_reads_back_as_its_source_and_identifier() {
        let r = reading();
        let hip = vec![("hipparcos-2007".to_owned(), "32754".to_owned())];
        assert_eq!(r.parse("HIP 32754"), hip);
        assert_eq!(r.parse("  hip 32754 "), hip);
        assert_eq!(
            r.parse("2MASX J00424433+4116074"),
            [("twomrs".to_owned(), "00424433+4116074".to_owned())]
        );
        assert_eq!(r.parse("HIP "), []);
        assert_eq!(r.parse("Sirius"), []);
    }

    #[test]
    fn designations_come_in_the_order_of_their_sources_ranks_then_unranked_by_source() {
        let all = vec![
            designation("mcconnachie-2012", "Canis Major"),
            designation("gaia-dr3-bright", "2947050466531873024"),
            designation("hipparcos-2007", "32754"),
            designation("hipparcos-2007", "32754"),
            designation("bailer-jones-2021", "2947050466531873024"),
        ];
        let own = all.iter().cloned().collect();
        let records = reading().designations(all, &own);
        let order: Vec<(&str, Option<&str>)> = records
            .iter()
            .map(|d| (d.source.as_str(), d.designation.as_deref()))
            .collect();
        assert_eq!(
            order,
            [
                ("hipparcos-2007", Some("HIP 32754")),
                ("gaia-dr3-bright", Some("Gaia DR3 2947050466531873024")),
                ("bailer-jones-2021", None),
                ("mcconnachie-2012", None),
            ]
        );
    }

    #[test]
    fn a_designation_a_source_gives_beside_an_entry_keeps_its_own_text() {
        // A Gaia–Hipparcos pair: its own source_id, and the HIP number kept as `HIP 32754`.
        let reading = Reading::new([(
            "gaia-dr3-hipparcos2".to_owned(),
            Some("Gaia DR3 {identifier}".to_owned()),
            Some(3),
        )]);
        let own = designation("gaia-dr3-hipparcos2", "2947050466531873024");
        let beside = designation("gaia-dr3-hipparcos2", "HIP 32754");
        let records = reading.designations(vec![beside, own.clone()], &BTreeSet::from([own]));
        let texts: Vec<Option<&str>> = records.iter().map(|d| d.designation.as_deref()).collect();
        assert_eq!(texts, [Some("Gaia DR3 2947050466531873024"), None]);
    }
}
