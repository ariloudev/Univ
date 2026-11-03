//! The first pass: entries that share an identifier of a published cross-identification scheme
//! are joined, unless the join would put two entries of one source together, which makes a
//! candidate link instead. An identifier that two or more entries of one source claim is applied
//! to none of them: each claimant is linked to every entry of another source that carries it,
//! so the outcome does not hang on the order of the entries.

use std::collections::HashMap;

use super::groups::Refused;
use super::rules::IdentifierForm;
use super::{Edge, Join, Work};

/// An identifier as compared: trimmed, and a number without its leading zeros, so that the
/// PGC number `     12` and `12` agree.
pub(super) fn normalized(identifier: &str) -> String {
    let t = identifier.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
        let n = t.trim_start_matches('0');
        if n.is_empty() { "0" } else { n }.to_owned()
    } else {
        t.to_owned()
    }
}

/// A common name as compared: its key in the names table (lower case, without white space),
/// then, for a catalogue designation of letters and a number, the number without its leading
/// zeros and `messier` written `m`. `NGC0055` and `NGC 55` agree, as do `MESSIER032` and `M32`.
pub(super) fn common_name(name: &str) -> String {
    let key = univ_model::release::rows::name_key(name);
    let split = key.find(|c: char| c.is_ascii_digit()).unwrap_or(key.len());
    let (letters, number) = key.split_at(split);
    if letters.is_empty()
        || !letters.chars().all(|c| c.is_ascii_alphabetic())
        || number.is_empty()
        || !number.bytes().all(|b| b.is_ascii_digit())
    {
        return key;
    }
    let letters = if letters == "messier" { "m" } else { letters };
    format!("{letters}{}", normalized(number))
}

/// Applies every scheme of the rules, in the order of the file, entries in their order.
pub(super) fn apply(work: &mut Work<'_>) {
    for (s, scheme) in work.rules.cross_identifications.iter().enumerate() {
        let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for (i, entry) in work.entries.iter().enumerate() {
            let Some(member) = scheme
                .members
                .iter()
                .find(|m| m.source == entry.designation.source)
            else {
                continue;
            };
            let Some(text) = member.field.read(entry) else {
                continue;
            };
            let id = match scheme.compare {
                IdentifierForm::Identifier => normalized(text),
                IdentifierForm::CommonName => common_name(text),
            };
            let list = by_id.entry(id.clone()).or_default();
            if list.is_empty() {
                order.push(id);
            }
            list.push(i);
        }
        for id in order {
            apply_one(work, s, &id, &by_id[&id]);
        }
    }
}

/// Applies the identifier `id` of scheme `scheme`, which the entries `list` carry.
fn apply_one(work: &mut Work<'_>, scheme: usize, id: &str, list: &[usize]) {
    let claims = |e: usize| {
        list.iter()
            .filter(|&&o| work.source_of[o] == work.source_of[e])
            .count()
    };
    let (sole, contested): (Vec<usize>, Vec<usize>) = list.iter().partition(|&&e| claims(e) == 1);
    let edge = |a: usize, b: usize| Edge {
        a,
        b,
        join: Join::Cross {
            scheme,
            identifier: id.to_owned(),
        },
    };
    let conflict = work.rules.confidence.cross_identification_conflict;
    if let Some((&first, rest)) = sole.split_first() {
        for &other in rest {
            if work.groups.find(first) == work.groups.find(other) {
                continue;
            }
            match work.groups.join(first, other) {
                Ok(()) => work.joins.push(edge(first, other)),
                Err(Refused::SharedSource | Refused::Linked) => {
                    work.groups.link(first, other);
                    work.links.push((edge(first, other), conflict));
                }
            }
        }
    }
    // Each claimant of a contested identifier, to every entry of another source carrying it.
    for &claimant in &contested {
        for &other in list {
            let apart = work.source_of[other] != work.source_of[claimant];
            // A pair of claimants of two contested sources is linked once.
            let once = !contested.contains(&other) || claimant < other;
            if apart && once && work.groups.find(claimant) != work.groups.find(other) {
                work.groups.link(claimant, other);
                work.links.push((edge(claimant, other), conflict));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{common_name, normalized};

    #[test]
    fn common_names_compare_by_key_and_catalogue_number() {
        assert_eq!(common_name("NGC0055"), common_name("NGC 55"));
        assert_eq!(common_name("MESSIER032"), common_name("M32"));
        assert_eq!(common_name("Fornax "), common_name("fornax"));
        assert_ne!(common_name("Sag dSph"), common_name("Sagittarius dSph"));
        assert_ne!(common_name("NGC 55"), common_name("NGC 555"));
        assert_eq!(common_name("And IV"), "andiv");
    }

    #[test]
    fn numbers_compare_without_leading_zeros_or_blanks() {
        assert_eq!(normalized("     12"), "12");
        assert_eq!(normalized("0012"), "12");
        assert_eq!(normalized("000"), "0");
        assert_eq!(normalized(" 00424433+4116074 "), "00424433+4116074");
    }
}
