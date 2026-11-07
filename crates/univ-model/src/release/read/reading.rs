//! How a release's sources are read (schema version 7): the form each source's catalogue writes
//! a designation in and the rank the designation rules give it, which order and format an
//! object's designations and names.

use std::collections::{BTreeSet, HashMap};

use super::columns::Columns;
use super::entries::source_of;
use super::{ReadError, Row, Vocabulary};
use crate::api::objects::{DesignationRecord, NameRecord};
use crate::api::sources::{IDENTIFIER, write_in};
use crate::ids::Designation;

/// Each source's designation form and rank, by the source's id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    forms: HashMap<String, String>,
    ranks: HashMap<String, u32>,
    /// Each form's text before and after its identifier, with its source: what a lookup's
    /// text is matched against.
    parts: Vec<(String, String, String)>,
}

impl Reading {
    /// The reading of sources given as their id, designation form and rank.
    pub fn new(sources: impl IntoIterator<Item = (String, Option<String>, Option<u32>)>) -> Self {
        let mut reading = Self::default();
        for (source, form, rank) in sources {
            if let Some(form) = form {
                if let Some((before, after)) = form.split_once(IDENTIFIER) {
                    reading
                        .parts
                        .push((source.clone(), before.to_owned(), after.to_owned()));
                }
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
        Some(write_in(self.forms.get(source)?, identifier))
    }

    /// The identifier `text` is in the form of each source, as the source and the identifier:
    /// `HIP 32754` is Hipparcos's `32754`. Letter case and white space are ignored in the form's
    /// own text, as a name's are, so `hip32754` reads the same.
    #[must_use]
    pub fn parse(&self, text: &str) -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = self
            .parts
            .iter()
            .filter_map(|(source, before, after)| {
                let rest = loosely(text.chars(), before.chars())
                    .map(|n| text.char_indices().nth(n).map_or("", |(at, _)| &text[at..]))?;
                let cut = loosely(rest.chars().rev(), after.chars().rev())?;
                let kept = rest.chars().count() - cut;
                let end = rest
                    .char_indices()
                    .nth(kept)
                    .map_or(rest.len(), |(at, _)| at);
                let identifier = rest[..end].trim();
                (!identifier.is_empty()).then(|| (source.clone(), identifier.to_owned()))
            })
            .collect();
        found.sort();
        found
    }

    /// The place of `source` in the order: the ranked first, by rank.
    fn rank(&self, source: &str) -> u32 {
        self.ranks.get(source).copied().unwrap_or(u32::MAX)
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
        // Once each, by source and identifier; the sort by rank below keeps that order within
        // a rank.
        let unique: BTreeSet<Designation> = designations.into_iter().collect();
        let mut records: Vec<DesignationRecord> = unique
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
        records.sort_by_cached_key(|r| self.rank(&r.source));
        records
    }
}

/// How many characters of `text` the literal `form` takes from its start, letter case and white
/// space ignored on both sides; none when `text` does not start with it.
fn loosely(text: impl Iterator<Item = char>, form: impl Iterator<Item = char>) -> Option<usize> {
    let mut form = form
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .peekable();
    let mut taken = 0;
    let mut text = text.peekable();
    while form.peek().is_some() {
        let c = text.next()?;
        taken += 1;
        if c.is_whitespace() {
            continue;
        }
        for lower in c.to_lowercase() {
            if form.next() != Some(lower) {
                return None;
            }
        }
    }
    // White space between the form's text and the identifier.
    while text.next_if(|c| c.is_whitespace()).is_some() {
        taken += 1;
    }
    Some(taken)
}

/// An object's `names` rows, each its source by number (`source`) and its `name`, in the order
/// of their sources' ranks, then by source and name, once each.
pub(super) fn names(v: &Vocabulary, rows: &[Row]) -> Result<Vec<NameRecord>, ReadError> {
    let unique = rows
        .iter()
        .map(|row| {
            let c = Columns::new(v, "names", row);
            Ok((source_of(v, &c)?.to_owned(), c.text("name")?.to_owned()))
        })
        .collect::<Result<BTreeSet<(String, String)>, ReadError>>()?;
    let mut names: Vec<NameRecord> = unique
        .into_iter()
        .map(|(source, name)| NameRecord { name, source })
        .collect();
    let reading = v.reading();
    names.sort_by_cached_key(|n| reading.rank(&n.source));
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
        assert_eq!(r.parse("HIP32754"), hip);
        assert_eq!(r.parse("H I P  32754"), hip);
        // A character whose lower case is longer leaves the cut where it belongs.
        assert_eq!(
            r.parse("HIP İ12"),
            [("hipparcos-2007".to_owned(), "İ12".to_owned())]
        );
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
