//! The designation rules: the order an object's designations are given in, by their sources,
//! so that a reader meets the designation astronomers use first, HIP 32754 before a Gaia
//! source id and NGC 4472 before a survey's coordinates. A release records the rules, and keeps
//! each source's place in them in `source_reading` (schema version 7).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;

/// The designation rules as `rules/designations.yaml` gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignationRules {
    /// What the order rests on, in words.
    pub statement: String,
    /// The sources, first given first. A source not listed comes after every listed one.
    pub order: Vec<SourceId>,
}

impl DesignationRules {
    /// The checks serde cannot make, as the field and the problem.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem: an empty statement or order, or a source listed twice.
    pub fn check(&self) -> Result<(), (String, String)> {
        if self.statement.trim().is_empty() {
            return Err(("statement".to_owned(), "is empty".to_owned()));
        }
        if self.order.is_empty() {
            return Err(("order".to_owned(), "lists no source".to_owned()));
        }
        let mut seen = HashSet::new();
        for (i, source) in self.order.iter().enumerate() {
            if !seen.insert(source) {
                return Err((format!("order[{i}]"), format!("repeats {source}")));
            }
        }
        Ok(())
    }

    /// The place of `source` in the order, 1 first; none for a source the order does not list.
    #[must_use]
    pub fn rank(&self, source: &SourceId) -> Option<u32> {
        let at = self.order.iter().position(|s| s == source)?;
        u32::try_from(at + 1).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(order: &[&str]) -> DesignationRules {
        DesignationRules {
            statement: "Hipparcos first.".to_owned(),
            order: order.iter().map(|s| SourceId::new(*s).unwrap()).collect(),
        }
    }

    #[test]
    fn a_source_ranks_by_its_place_in_the_order_and_an_unlisted_one_has_none() {
        let r = rules(&["hipparcos-2007", "gaia-dr3-bright"]);
        r.check().unwrap();
        assert_eq!(r.rank(&SourceId::new("hipparcos-2007").unwrap()), Some(1));
        assert_eq!(r.rank(&SourceId::new("gaia-dr3-bright").unwrap()), Some(2));
        assert_eq!(r.rank(&SourceId::new("twomrs").unwrap()), None);
    }

    #[test]
    fn rules_with_no_statement_no_order_or_a_source_twice_are_refused() {
        let mut r = rules(&["twomrs"]);
        r.statement = " ".to_owned();
        assert_eq!(r.check().unwrap_err().0, "statement");
        assert_eq!(rules(&[]).check().unwrap_err().0, "order");
        assert_eq!(
            rules(&["twomrs", "twomrs"]).check().unwrap_err().0,
            "order[1]"
        );
    }

    #[test]
    fn the_repository_rules_are_read_and_rank_a_stars_iau_name_then_its_hip_number_first() {
        let file =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/designations.yaml");
        let text = std::fs::read_to_string(&file).unwrap();
        let r: DesignationRules =
            crate::yaml::parse_checked(&file, &text, DesignationRules::check).unwrap();
        assert_eq!(r.rank(&SourceId::new("iau-star-names").unwrap()), Some(1));
        assert_eq!(r.rank(&SourceId::new("hipparcos-2007").unwrap()), Some(2));
        // Every source it ranks is defined: a mistyped id would leave its source unranked.
        let sources = file.parent().unwrap().join("../sources");
        for source in &r.order {
            let defined = sources.join(format!("{source}.yaml"));
            assert!(defined.is_file(), "{source} names no source definition");
        }
    }
}
