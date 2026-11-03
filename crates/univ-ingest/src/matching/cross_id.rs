//! The first pass: entries that share an identifier of a published cross-identification scheme
//! are joined, unless the join would put two entries of one source together, which makes a
//! candidate link instead. An identifier that two or more entries of one source claim is applied
//! to none of them: each claimant is linked to every entry of another source that carries it,
//! so the outcome does not hang on the order of the entries.

use std::collections::HashMap;

use super::groups::Refused;
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
            let id = normalized(text);
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
    use super::normalized;

    #[test]
    fn numbers_compare_without_leading_zeros_or_blanks() {
        assert_eq!(normalized("     12"), "12");
        assert_eq!(normalized("0012"), "12");
        assert_eq!(normalized("000"), "0");
        assert_eq!(normalized(" 00424433+4116074 "), "00424433+4116074");
    }
}
