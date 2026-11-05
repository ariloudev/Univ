//! The last pass: each row of a galaxy-group source related to the galaxy its source names as
//! the group's dominant member, by an identifier of a cross-identification scheme, and to each
//! galaxy a galaxy source assigns to it. The group and the galaxy stay apart; the relation lends
//! neither a measurement.

use std::collections::HashMap;

use super::Work;
use super::rules::GalaxyGroupRule;
use crate::table::identifier_key;

/// A relation from the group entry `group` to the galaxy entry `galaxy`, by the identifier
/// `identifier` of scheme number `scheme`: the galaxy dominates the group, or is a member.
pub(super) struct GroupRelation {
    pub dominant: bool,
    pub group: usize,
    pub galaxy: usize,
    pub scheme: usize,
    pub identifier: String,
}

/// Every relation of a group entry to an entry of another source that carries, in the group
/// rule's scheme, the identifier the group names its dominant galaxy by.
pub(super) fn find(work: &Work<'_>) -> Vec<GroupRelation> {
    let rules = work.rules;
    let mut found = Vec::new();
    for group in &rules.galaxy_groups {
        let Some(scheme) = rules
            .cross_identifications
            .iter()
            .position(|s| s.scheme == group.scheme)
        else {
            continue;
        };
        let members = &rules.cross_identifications[scheme].members;
        let mut carriers: HashMap<String, Vec<usize>> = HashMap::new();
        for (e, entry) in work.entries.iter().enumerate() {
            let source = &entry.designation.source;
            if source == &group.source {
                continue;
            }
            for m in members.iter().filter(|m| &m.source == source) {
                if let Some(text) = m.field.read(entry) {
                    carriers
                        .entry(identifier_key(text).to_owned())
                        .or_default()
                        .push(e);
                }
            }
        }
        for (g, entry) in work.entries.iter().enumerate() {
            if entry.designation.source != group.source {
                continue;
            }
            let Some(id) = group.dominant_member.read(entry).map(identifier_key) else {
                continue;
            };
            for &galaxy in carriers.get(id).map_or(&[][..], Vec::as_slice) {
                found.push(GroupRelation {
                    dominant: true,
                    group: g,
                    galaxy,
                    scheme,
                    identifier: id.to_owned(),
                });
            }
        }
        member_relations(work, group, scheme, &mut found);
    }
    found
}

/// The relation of each galaxy of a member source to the group entry whose dominant-member
/// identifier its field gives.
fn member_relations(
    work: &Work<'_>,
    rule: &GalaxyGroupRule,
    scheme: usize,
    found: &mut Vec<GroupRelation>,
) {
    if rule.members.is_empty() {
        return;
    }
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (g, entry) in work.entries.iter().enumerate() {
        if entry.designation.source == rule.source
            && let Some(id) = rule.dominant_member.read(entry)
        {
            groups
                .entry(identifier_key(id).to_owned())
                .or_default()
                .push(g);
        }
    }
    for (e, entry) in work.entries.iter().enumerate() {
        let source = &entry.designation.source;
        for m in rule.members.iter().filter(|m| &m.source == source) {
            let Some(id) = m.field.read(entry).map(identifier_key) else {
                continue;
            };
            for &group in groups.get(id).map_or(&[][..], Vec::as_slice) {
                found.push(GroupRelation {
                    dominant: false,
                    group,
                    galaxy: e,
                    scheme,
                    identifier: id.to_owned(),
                });
            }
        }
    }
}
