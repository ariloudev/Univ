//! The last step: the groups made into objects, each entry with how it joined, its
//! designations, common names and candidate links.

use std::collections::VecDeque;

use univ_model::entry::CatalogueEntry;
use univ_model::ids::Designation;
use univ_model::object::{
    CandidateLink, CommonName, Confidence, MatchRecord, Object, ObjectEntry, ObjectKind, Relation,
    RelationKind, Resolution,
};

use super::relations::GroupRelation;
use super::rules::MatchingRules;
use super::{Edge, Join, MatchReport, Matched, Work};

/// An object's entries, its anchor first and each other entry with the join that reached it:
/// the entry it joined through, and the join.
type Members = Vec<(usize, Option<(usize, Join)>)>;

/// The objects to build: the members of each, by entry, and the joins and links between them.
pub(super) struct Plan {
    objects: Vec<Members>,
    links: Vec<(Edge, f64)>,
}

pub(super) fn plan(work: &mut Work<'_>) -> Plan {
    let n = work.entries.len();
    let mut adjacent: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (k, e) in work.joins.iter().enumerate() {
        adjacent[e.a].push((e.b, k));
        adjacent[e.b].push((e.a, k));
    }
    let mut seen = vec![false; n];
    let mut objects = Vec::new();
    for anchor in 0..n {
        if seen[anchor] {
            continue;
        }
        // Breadth first from the anchor, the first entry of its group in order.
        seen[anchor] = true;
        let mut members = vec![(anchor, None)];
        let mut queue = VecDeque::from([anchor]);
        while let Some(e) = queue.pop_front() {
            for &(other, k) in &adjacent[e] {
                if !seen[other] {
                    seen[other] = true;
                    members.push((other, Some((e, work.joins[k].join.clone()))));
                    queue.push_back(other);
                }
            }
        }
        objects.push(members);
    }
    Plan {
        objects,
        links: std::mem::take(&mut work.links),
    }
}

/// The resolution of an entry that joined through `via` by `join`.
fn resolution(rules: &MatchingRules, via: &Designation, join: Join) -> Resolution {
    match join {
        Join::Cross { scheme, identifier } => Resolution::CrossIdentification {
            scheme: rules.cross_identifications[scheme].scheme.clone(),
            identifier,
            via: via.clone(),
            confidence: Confidence::CERTAIN,
        },
        Join::Position(c) => Resolution::Match(MatchRecord {
            method: c.method,
            with: via.clone(),
            separation_arcsec: c.separation_arcsec,
            difference: c.difference,
            confidence: confidence(c.confidence),
        }),
    }
}

/// `value` as a confidence, clamped to [0, 1]; a value that is not a number gives no confidence.
fn confidence(value: f64) -> Confidence {
    Confidence::new(value.clamp(0.0, 1.0)).unwrap_or(Confidence::NONE)
}

pub(super) fn build(
    rules: &MatchingRules,
    entries: Vec<CatalogueEntry>,
    plan: Plan,
    relations: Vec<GroupRelation>,
) -> Matched {
    let designations: Vec<Designation> = entries.iter().map(|e| e.designation.clone()).collect();
    let mut slots: Vec<Option<CatalogueEntry>> = entries.into_iter().map(Some).collect();
    let mut object_of = vec![0; slots.len()];
    let mut report = MatchReport {
        entries: slots.len() as u64,
        ..MatchReport::default()
    };
    let mut objects = Vec::with_capacity(plan.objects.len());
    for (o, members) in plan.objects.into_iter().enumerate() {
        let mut list = Vec::with_capacity(members.len());
        for (e, via) in members {
            object_of[e] = o;
            let resolved_by = match via {
                None => Resolution::Anchor,
                Some((through, join)) => {
                    match join {
                        Join::Cross { .. } => report.cross_identified += 1,
                        Join::Position(_) => report.merged_by_position += 1,
                    }
                    resolution(rules, &designations[through], join)
                }
            };
            if let Some(entry) = slots[e].take() {
                list.push(ObjectEntry {
                    entry,
                    resolved_by: resolved_by.into(),
                });
            }
        }
        if list.len() > 1 {
            report.multi_entry_objects += 1;
        }
        let mut object = Object::new(list);
        if object.sources().any(|s| rules.galaxy_group(s).is_some()) {
            object.kind = ObjectKind::GalaxyGroup;
        }
        attach_names(rules, &mut object);
        objects.push(object);
    }
    for (edge, c) in plan.links {
        let (oa, ob) = (object_of[edge.a], object_of[edge.b]);
        if oa == ob {
            continue;
        }
        report.candidate_links += 1;
        let reason = resolution(rules, &designations[edge.b], edge.join);
        let back = match &reason {
            Resolution::Match(m) => Resolution::Match(MatchRecord {
                with: designations[edge.a].clone(),
                ..m.clone()
            }),
            Resolution::CrossIdentification {
                scheme,
                identifier,
                confidence,
                ..
            } => Resolution::CrossIdentification {
                scheme: scheme.clone(),
                identifier: identifier.clone(),
                via: designations[edge.a].clone(),
                confidence: *confidence,
            },
            Resolution::Anchor => Resolution::Anchor,
        };
        for (o, from, to, why) in [(oa, edge.a, edge.b, reason), (ob, edge.b, edge.a, back)] {
            objects[o].links.push(CandidateLink {
                entry: designations[from].clone(),
                candidate: designations[to].clone(),
                reason: why,
                confidence: confidence(c),
            });
        }
    }
    for r in relations {
        let (og, ox) = (object_of[r.group], object_of[r.galaxy]);
        if og == ox {
            continue;
        }
        report.group_relations += 1;
        let scheme = &rules.cross_identifications[r.scheme].scheme;
        let (of_group, of_galaxy) = if r.dominant {
            (RelationKind::DominantMember, RelationKind::DominantMemberOf)
        } else {
            (RelationKind::Member, RelationKind::MemberOf)
        };
        for (o, kind, from, to) in [
            (og, of_group, r.group, r.galaxy),
            (ox, of_galaxy, r.galaxy, r.group),
        ] {
            objects[o].relations.push(Relation {
                kind,
                entry: designations[from].clone(),
                other: designations[to].clone(),
                scheme: scheme.clone(),
                identifier: r.identifier.clone(),
            });
        }
    }
    report.objects = objects.len() as u64;
    Matched { objects, report }
}

/// The designations and common names the object's entries carry beside their own, each with
/// the source that gave it.
fn attach_names(rules: &MatchingRules, object: &mut Object) {
    let mut given = Vec::new();
    for e in &object.entries {
        let entry = &e.entry;
        let source = &entry.designation.source;
        for rule in rules.designations.iter().filter(|r| &r.source == source) {
            if let Some(text) = rule.field.read(entry)
                && let Ok(d) = Designation::new(source.clone(), format!("{}{text}", rule.prefix))
                && !object.designations().any(|o| *o == d)
                && !given.contains(&d)
            {
                given.push(d);
            }
        }
        for rule in rules.names.iter().filter(|r| &r.source == source) {
            if let Some(text) = rule.field.read(entry) {
                let name = CommonName {
                    name: text.to_owned(),
                    source: source.clone(),
                };
                if !object.names.contains(&name) {
                    object.names.push(name);
                }
            }
        }
    }
    object.given.extend(given);
}

#[cfg(test)]
mod tests {
    use super::confidence;

    #[test]
    fn a_confidence_that_is_not_a_number_is_none() {
        assert_eq!(confidence(f64::NAN).get(), 0.0);
        assert_eq!(confidence(1.5).get(), 1.0);
        assert_eq!(confidence(-0.5).get(), 0.0);
        assert_eq!(confidence(0.25).get(), 0.25);
    }
}
