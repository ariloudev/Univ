//! The order of an object's redshift distances among themselves: a measured redshift first,
//! then the smallest published velocity uncertainty, a redshift without one after those with
//! one and given none, then the method rank, then the rule's order of sources, then the order
//! of the entries. Only the first of them, the leading redshift distance, competes with the
//! object's other candidates.
//!
//! Fractional distance uncertainty cannot order redshift distances: the peculiar-velocity term
//! is the same velocity for each, so it is a smaller fraction of the higher redshift, which then
//! always wins, however its velocity was obtained.

use std::cmp::Ordering;

use univ_model::object::{Decided, Object};
use univ_model::value::KindOfValue;

use super::distance::{Candidate, is_redshift};
use super::rules::RankingRules;

/// Whether `a` orders before `b`, both redshift candidates by place in `candidates`, and the
/// first step that set them apart.
fn order(
    object: &Object,
    candidates: &[Candidate],
    rules: &RankingRules,
    a: usize,
    b: usize,
) -> (Ordering, Decided) {
    let (ca, cb) = (&candidates[a], &candidates[b]);
    let not_measured = |c: &Candidate| c.kind_of_value != KindOfValue::Measured;
    // A redshift without a published velocity uncertainty after every one with one, never
    // given one.
    let sigma = |x: &Candidate, y: &Candidate| match (x.velocity_sigma_km_s, y.velocity_sigma_km_s)
    {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (x, y) => x.is_none().cmp(&y.is_none()),
    };
    let source = |c: &Candidate| {
        let entry = &object.entries[c.entry].entry;
        rules.source_place(&entry.designation.source)
    };
    let steps = [
        (
            not_measured(ca).cmp(&not_measured(cb)),
            Decided::KindOfValue,
        ),
        (sigma(ca, cb), Decided::VelocityUncertainty),
        (
            rules.rank_of(ca.rank).cmp(&rules.rank_of(cb.rank)),
            Decided::MethodRank,
        ),
        (source(ca).cmp(&source(cb)), Decided::SourceOrder),
    ];
    steps
        .into_iter()
        .find(|(o, _)| o.is_ne())
        .unwrap_or((a.cmp(&b), Decided::EntryOrder))
}

/// The leading redshift candidate among `open`, by place in `candidates`.
pub(super) fn leading(
    object: &Object,
    candidates: &[Candidate],
    open: &[usize],
    rules: &RankingRules,
) -> Option<usize> {
    open.iter()
        .copied()
        .filter(|&i| is_redshift(candidates[i].rank))
        .min_by(|&a, &b| order(object, candidates, rules, a, b).0)
}

/// The step that set the redshift candidate `best` before the redshift candidate `next`.
pub(super) fn decided(
    object: &Object,
    candidates: &[Candidate],
    rules: &RankingRules,
    best: usize,
    next: usize,
) -> Decided {
    order(object, candidates, rules, best, next).1
}
