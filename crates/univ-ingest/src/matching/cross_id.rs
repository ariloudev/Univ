//! The first pass: entries that share an identifier of a published cross-identification scheme
//! are joined, unless the join would put two entries of one source together, which makes a
//! candidate link instead.

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
            let list = &by_id[&id];
            let Some((&first, rest)) = list.split_first() else {
                continue;
            };
            for &other in rest {
                let edge = Edge {
                    a: first,
                    b: other,
                    join: Join::Cross {
                        scheme: s,
                        identifier: id.clone(),
                    },
                };
                if work.groups.find(first) == work.groups.find(other) {
                    continue;
                }
                match work.groups.join(first, other) {
                    Ok(()) => work.joins.push(edge),
                    Err(Refused::SharedSource | Refused::Linked) => {
                        work.groups.link(first, other);
                        let c = work.rules.confidence.cross_identification_conflict;
                        work.links.push((edge, c));
                    }
                }
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
