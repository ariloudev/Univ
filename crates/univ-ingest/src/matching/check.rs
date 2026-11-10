//! The checks of the matching rules that serde cannot make, apart from the rules they check.

use std::collections::BTreeSet;

use super::rules::{LineOfSightRule, MatchingRules, key};

impl MatchingRules {
    /// The checks serde cannot make: positive tolerances, factors within [0, 1], each pair and
    /// each scheme member given once.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem.
    pub fn check(&self) -> Result<(), (String, String)> {
        let c = &self.confidence;
        if !(c.exponent.is_finite() && c.exponent > 0.0) {
            return Err(field("confidence.exponent", "is not positive"));
        }
        for (name, v) in [
            ("unchecked_factor", c.unchecked_factor),
            (
                "cross_identification_conflict",
                c.cross_identification_conflict,
            ),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(field(&format!("confidence.{name}"), "lies outside [0, 1]"));
            }
        }
        self.check_pairs()?;
        if let Some(extents) = &self.extents {
            extents.check()?;
        }
        if !positive(self.structure_members.angular_tolerance_arcsec) {
            return Err(field(
                "structure_members.angular_tolerance_arcsec",
                "is not positive",
            ));
        }
        for (i, group) in self.galaxy_groups.iter().enumerate() {
            if self.scheme(&group.scheme).is_none() {
                return Err(field(
                    &format!("galaxy_groups[{i}].scheme"),
                    &format!(
                        "names {}, which no cross-identification defines",
                        group.scheme
                    ),
                ));
            }
            if self.pairs.iter().any(|p| p.sources.contains(&group.source)) {
                return Err(field(
                    &format!("galaxy_groups[{i}].source"),
                    "is a source of groups, which a pair would match to galaxies by position",
                ));
            }
        }
        for (i, scheme) in self.cross_identifications.iter().enumerate() {
            if !scheme.aliases.is_empty()
                && scheme
                    .aliases_basis
                    .as_deref()
                    .is_none_or(|b| b.trim().is_empty())
            {
                return Err(field(
                    &format!("cross_identifications[{i}].aliases"),
                    "are given without their aliases_basis",
                ));
            }
            if let Some(t) = scheme.angular_tolerance_arcmin {
                let at = format!("cross_identifications[{i}].angular_tolerance_arcmin");
                if !(t.is_finite() && t > 0.0) {
                    return Err(field(&at, "is not a positive number"));
                }
                if scheme
                    .tolerance_basis
                    .as_deref()
                    .is_none_or(|b| b.trim().is_empty())
                {
                    return Err(field(&at, "is given without its tolerance_basis"));
                }
            }
            for (j, m) in scheme.members.iter().enumerate() {
                if scheme.members[..j]
                    .iter()
                    .any(|o| o.source == m.source && o.field == m.field)
                {
                    return Err(field(
                        &format!("cross_identifications[{i}].members"),
                        &format!("names {} by one field twice", m.source),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Each pair given once, of two sources, with positive tolerances.
    fn check_pairs(&self) -> Result<(), (String, String)> {
        let mut seen = BTreeSet::new();
        for (i, pair) in self.pairs.iter().enumerate() {
            let at = |what: &str| format!("pairs[{i}].{what}");
            let [a, b] = &pair.sources;
            if a == b {
                return Err(field(&at("sources"), "names one source twice"));
            }
            if !seen.insert(key(a, b)) {
                return Err(field(&at("sources"), "is a pair given before"));
            }
            if !positive(pair.angular_tolerance_arcsec) {
                return Err(field(&at("angular_tolerance_arcsec"), "is not positive"));
            }
            if pair
                .nearby
                .as_ref()
                .is_some_and(|n| !n.widens(pair.angular_tolerance_arcsec))
            {
                return Err(field(
                    &at("nearby"),
                    "needs a positive velocity, a tolerance wider than the pair's and a basis",
                ));
            }
            match pair.line_of_sight {
                LineOfSightRule::Velocity { tolerance_km_s: t }
                | LineOfSightRule::Distance {
                    tolerance_fraction: t,
                } if !positive(t) => {
                    return Err(field(&at("line_of_sight"), "has a tolerance not positive"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

fn field(name: &str, problem: &str) -> (String, String) {
    (name.to_owned(), problem.to_owned())
}
