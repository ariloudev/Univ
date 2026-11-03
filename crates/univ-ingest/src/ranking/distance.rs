//! The preferred distance: every candidate measurement with its fractional uncertainty, and the
//! one the rule chooses.

use std::cmp::Ordering;

use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::derive::{Release, inverted_parallax, peculiar_velocity_redshift, redshift_sigma};
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, Object, PreferredDistance,
};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use super::redshift;
use super::rules::RankingRules;

/// The probability of a one-sigma interval, which a credible interval must have for half its
/// width to stand for a sigma.
const ONE_SIGMA: f64 = 0.6827;

/// A measurement that could place the object: the place of its entry in the object, which of
/// the entry's measurements it is, and how it ranks.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub entry: usize,
    pub which: Which,
    pub rank: MethodRank,
    pub distance_mpc: f64,
    pub uncertainty: FractionalUncertainty,
    /// Whether its value was measured, estimated, inferred or reconstructed.
    pub kind_of_value: KindOfValue,
    /// A redshift's published one-sigma velocity error, c times the error of the redshift as
    /// published, km/s; none for a redshift published without one, and for any other candidate.
    pub velocity_sigma_km_s: Option<f64>,
}

/// Which measurement of an entry a candidate is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Distance(usize),
    Redshift(usize),
    Parallax,
}

impl Candidate {
    /// The candidate as the object records it, by its entry's designation.
    #[must_use]
    pub fn measurement(&self, object: &Object) -> MeasurementRef {
        let entry = object.entries[self.entry].entry.designation.clone();
        match self.which {
            Which::Distance(index) => MeasurementRef::Distance { entry, index },
            Which::Redshift(index) => MeasurementRef::Redshift { entry, index },
            Which::Parallax => MeasurementRef::Parallax { entry },
        }
    }
}

/// The comoving distance in Mpc of redshift `z` under the release cosmology, read from the
/// release's table of D(z), and its derivative dD/dz = c / H(z). The one place redshift distances
/// are computed in ranking.
#[must_use]
pub fn redshift_distance(release: &Release, z: f64) -> Option<(f64, f64)> {
    let table = release.table();
    let (d, slope) = (table.distance(z).ok()?, table.slope(z).ok()?);
    (d > 0.0).then_some((d, slope))
}

/// One sigma of `q` over its value: a symmetric error, the mean side of an asymmetric interval,
/// or half the width of a one-sigma credible interval; none for any other form.
#[must_use]
pub fn fraction(q: &Quantity) -> Option<f64> {
    let sigma = sigma(q.uncertainty)?;
    (q.value > 0.0).then(|| sigma / q.value)
}

fn sigma(u: Uncertainty) -> Option<f64> {
    match u {
        Uncertainty::Symmetric { sigma } => Some(sigma),
        Uncertainty::Asymmetric { minus, plus } => Some(minus.midpoint(plus)),
        Uncertainty::CredibleInterval {
            minus,
            plus,
            probability,
        } if (probability - ONE_SIGMA).abs() < 0.01 => Some(minus.midpoint(plus)),
        _ => None,
    }
}

fn reported(total: f64, measurement: Option<f64>, pv: Option<f64>) -> FractionalUncertainty {
    FractionalUncertainty::Reported {
        total,
        measurement,
        peculiar_velocity: pv,
    }
}

/// Every candidate of `object`: its entries' distances, parallaxes fine enough to invert once
/// their zero point is taken off, and positive redshifts, in the order of the entries.
#[must_use]
pub fn candidates(object: &Object, rules: &RankingRules, release: &Release) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (at, e) in object.entries.iter().enumerate() {
        let entry = &e.entry;
        for (index, d) in entry.distances.iter().enumerate() {
            let Ok(mpc) = d.normalized.to(Unit::Megaparsec) else {
                continue;
            };
            if mpc.value <= 0.0 {
                continue;
            }
            out.push(Candidate {
                entry: at,
                which: Which::Distance(index),
                rank: d.kind.into(),
                distance_mpc: mpc.value,
                uncertainty: fraction(&mpc).map_or(FractionalUncertainty::NoneReported, |f| {
                    reported(f, Some(f), None)
                }),
                kind_of_value: d.kind_of_value,
                velocity_sigma_km_s: None,
            });
        }
        let zero_point = rules.zero_point_of(&entry.designation.source);
        if let Some(p) = &entry.parallax
            && let Ok(mas) = p.normalized.to(Unit::Milliarcsecond)
            && let Some(inverted) = inverted_parallax(mas.value, sigma_of(&mas), zero_point)
            && let Some(f) = inverted.fraction()
            && f <= rules.parallax_max_fractional_error
        {
            out.push(Candidate {
                entry: at,
                which: Which::Parallax,
                rank: MethodRank::Geometric,
                distance_mpc: inverted.distance_pc * 1e-6,
                uncertainty: reported(f, Some(f), None),
                kind_of_value: p.kind,
                velocity_sigma_km_s: None,
            });
        }
        for (index, r) in entry.redshifts.iter().enumerate() {
            let z = r.cmb.value.value;
            let Some((d, slope)) = redshift_distance(release, z) else {
                continue;
            };
            // A redshift distance's uncertainty always holds the rule's peculiar-velocity term,
            // the line-of-sight velocity a redshift does not separate from the Hubble flow: in
            // quadrature with the redshift's one-sigma error where one is published, alone
            // where none is. The term is the rule's stated one, not an invented error, so even a
            // redshift published without an error ranks as a candidate with an uncertainty.
            let v = rules.peculiar_velocity_km_s;
            let sigma_z = sigma(r.cmb.value.uncertainty);
            let uncertainty = reported(
                slope * redshift_sigma(z, sigma_z.unwrap_or(0.0), v) / d,
                sigma_z.map(|s| slope * s / d),
                Some(slope * peculiar_velocity_redshift(z, v) / d),
            );
            out.push(Candidate {
                entry: at,
                which: Which::Redshift(index),
                rank: r.kind.into(),
                distance_mpc: d,
                uncertainty,
                kind_of_value: r.kind_of_value,
                velocity_sigma_km_s: sigma(r.cmb.as_published().uncertainty)
                    .map(|s| s * SPEED_OF_LIGHT),
            });
        }
    }
    out
}

/// The symmetric error of `q`, where it has one.
fn sigma_of(q: &Quantity) -> Option<f64> {
    match q.uncertainty {
        Uncertainty::Symmetric { sigma } => Some(sigma),
        _ => None,
    }
}

/// Whether `rank` is a redshift's.
pub(super) fn is_redshift(rank: MethodRank) -> bool {
    matches!(
        rank,
        MethodRank::SpectroscopicRedshift | MethodRank::PhotometricRedshift
    )
}

/// The candidates by place in `candidates`, leaving out those in `excluded`.
fn open(candidates: &[Candidate], excluded: &[usize]) -> Vec<usize> {
    (0..candidates.len())
        .filter(|i| !excluded.contains(i))
        .collect()
}

/// The candidates the rule weighs, by place in `candidates`, leaving out those in `excluded`:
/// every geometric and indicator candidate, and the leading redshift candidate unless
/// `threshold` is given, a geometric or indicator candidate exists, and its fractional
/// uncertainty exceeds it.
fn remaining(
    object: &Object,
    candidates: &[Candidate],
    excluded: &[usize],
    threshold: Option<f64>,
    rules: &RankingRules,
) -> Vec<usize> {
    let open = open(candidates, excluded);
    let lead = redshift::leading(object, candidates, &open, rules);
    let indicator = open.iter().any(|&i| !is_redshift(candidates[i].rank));
    open.into_iter()
        .filter(|&i| {
            let c = &candidates[i];
            let above = threshold.is_some_and(|t| c.uncertainty.total().is_some_and(|f| f > t));
            !is_redshift(c.rank) || (Some(i) == lead && !(indicator && above))
        })
        .collect()
}

/// The winner among `open` by steps (ii) and (iii) of the rule, and what decided it: the
/// comparison that set the winner before the candidate ordered next, the last one the rule
/// needed.
fn among(
    object: &Object,
    candidates: &[Candidate],
    open: &[usize],
    rules: &RankingRules,
) -> Option<(usize, Decided)> {
    let total = |i: usize| candidates[i].uncertainty.total();
    let rank = |i: usize| rules.rank_of(candidates[i].rank);
    let (reported, unreported): (Vec<usize>, Vec<usize>) =
        open.iter().partition(|&&i| total(i).is_some());
    let best_reported = reported.iter().map(|&i| rank(i)).min();
    let mut qualifying: Vec<usize> = unreported
        .into_iter()
        .filter(|&i| best_reported.is_none_or(|best| rank(i) < best))
        .collect();
    if !qualifying.is_empty() {
        let source = |i: usize| {
            let entry = &object.entries[candidates[i].entry].entry;
            rules.source_place(&entry.designation.source)
        };
        qualifying.sort_by(|&a, &b| {
            rank(a)
                .cmp(&rank(b))
                .then(source(a).cmp(&source(b)))
                .then(a.cmp(&b))
        });
        let best = qualifying[0];
        let decided = match qualifying.get(1) {
            None => Decided::MethodRankNoneReported,
            Some(&next) if rank(next) != rank(best) => Decided::MethodRankNoneReported,
            Some(&next) if source(next) != source(best) => Decided::SourceOrder,
            Some(_) => Decided::EntryOrder,
        };
        return Some((best, decided));
    }
    let smallest = reported
        .iter()
        .filter_map(|&i| total(i))
        .fold(f64::INFINITY, f64::min);
    let mut tied: Vec<usize> = reported
        .into_iter()
        .filter(|&i| total(i).is_some_and(|t| t <= smallest + rules.tie_tolerance))
        .collect();
    let t = |i: usize| total(i).unwrap_or(f64::INFINITY);
    tied.sort_by(|&a, &b| {
        rank(a)
            .cmp(&rank(b))
            .then(t(a).partial_cmp(&t(b)).unwrap_or(Ordering::Equal))
            .then(a.cmp(&b))
    });
    let best = *tied.first()?;
    let decided = match tied.get(1) {
        None => Decided::Uncertainty,
        Some(&next) if rank(next) != rank(best) => Decided::MethodRank,
        Some(&next) if t(next) > t(best) => Decided::Uncertainty,
        Some(_) => Decided::EntryOrder,
    };
    Some((best, decided))
}

/// The candidate the rule prefers, by place in `candidates`, leaving out those in `excluded`,
/// and what decided it: the redshift threshold where the winner without it would have been
/// another; and, for a redshift whose next candidate is a redshift too, the step of the
/// redshifts' own order that set them apart.
fn winner(
    object: &Object,
    candidates: &[Candidate],
    excluded: &[usize],
    rules: &RankingRules,
) -> Option<(usize, Decided)> {
    let weighed = |threshold| remaining(object, candidates, excluded, threshold, rules);
    let open = weighed(Some(rules.redshift_threshold));
    let (best, decided) = among(object, candidates, &open, rules)?;
    if self::open(candidates, excluded).len() == 1 {
        return Some((best, Decided::OnlyCandidate));
    }
    let unthresholded = weighed(None);
    if unthresholded.len() != open.len()
        && among(object, candidates, &unthresholded, rules).is_some_and(|(b, _)| b != best)
    {
        return Some((best, Decided::RedshiftThreshold));
    }
    if is_redshift(candidates[best].rank) {
        let without: Vec<usize> = excluded.iter().copied().chain([best]).collect();
        let next = remaining(
            object,
            candidates,
            &without,
            Some(rules.redshift_threshold),
            rules,
        );
        if let Some((next, _)) = among(object, candidates, &next, rules)
            && is_redshift(candidates[next].rank)
        {
            return Some((
                best,
                redshift::decided(object, candidates, rules, best, next),
            ));
        }
    }
    Some((best, decided))
}

/// The preferred distance of `object` among `candidates`, with the best of the others as
/// runner-up.
#[must_use]
pub fn choose(
    object: &Object,
    candidates: &[Candidate],
    rules: &RankingRules,
) -> Option<PreferredDistance> {
    let (best, decided) = winner(object, candidates, &[], rules)?;
    let runner_up = winner(object, candidates, &[best], rules).map(|(i, _)| {
        let c = &candidates[i];
        (c.measurement(object), c.uncertainty)
    });
    let c = &candidates[best];
    Some(PreferredDistance {
        measurement: c.measurement(object),
        rank: c.rank,
        distance_mpc: c.distance_mpc,
        uncertainty: c.uncertainty,
        decided,
        runner_up,
    })
}
