//! The preferred distance: every candidate measurement with its fractional uncertainty, and the
//! one the rule chooses.

use std::cmp::Ordering;

use univ_model::cosmology::{Cosmology, SPEED_OF_LIGHT};
use univ_model::entry::RedshiftKind;
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, Object, PreferredDistance,
};
use univ_model::units::Unit;
use univ_model::value::{Quantity, Uncertainty};

use super::rules::RankingRules;

/// The probability of a one-sigma interval, which a credible interval must have for half its
/// width to stand for a sigma.
const ONE_SIGMA: f64 = 0.6827;

/// A measurement that could place the object.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub measurement: MeasurementRef,
    pub rank: MethodRank,
    pub distance_mpc: f64,
    pub uncertainty: FractionalUncertainty,
}

/// The comoving distance in Mpc of redshift `z` under `cosmology`, and its derivative dD/dz.
/// The one place redshift distances are computed, so a tabulated D(z) can take its place.
#[must_use]
pub fn redshift_distance(cosmology: &Cosmology, z: f64) -> Option<(f64, f64)> {
    let d = cosmology.comoving_distance(z).ok()?;
    let h = 1e-4 * (1.0 + z);
    let (lo, hi) = ((z - h).max(0.0), z + h);
    let slope =
        (cosmology.comoving_distance(hi).ok()? - cosmology.comoving_distance(lo).ok()?) / (hi - lo);
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

/// Every candidate of `object`: its entries' distances, parallaxes fine enough to invert, and
/// positive redshifts, in the order of the entries.
#[must_use]
pub fn candidates(object: &Object, rules: &RankingRules, cosmology: &Cosmology) -> Vec<Candidate> {
    let mut out = Vec::new();
    for e in &object.entries {
        let entry = &e.entry;
        let at = &entry.designation;
        for (index, d) in entry.distances.iter().enumerate() {
            let Ok(mpc) = d.normalized.to(Unit::Megaparsec) else {
                continue;
            };
            if mpc.value <= 0.0 {
                continue;
            }
            out.push(Candidate {
                measurement: MeasurementRef::Distance {
                    entry: at.clone(),
                    index,
                },
                rank: d.kind.into(),
                distance_mpc: mpc.value,
                uncertainty: fraction(&mpc).map_or(FractionalUncertainty::NoneReported, |f| {
                    reported(f, Some(f), None)
                }),
            });
        }
        if let Some(p) = &entry.parallax
            && let Some(f) = fraction(&p.normalized)
            && f <= rules.parallax_max_fractional_error
        {
            let arcsec = p.normalized.value * 3_600.0;
            out.push(Candidate {
                measurement: MeasurementRef::Parallax { entry: at.clone() },
                rank: MethodRank::Geometric,
                distance_mpc: 1e-6 / arcsec,
                uncertainty: reported(f, Some(f), None),
            });
        }
        for (index, r) in entry.redshifts.iter().enumerate() {
            let z = r.cmb.value.value;
            let Some((d, slope)) = redshift_distance(cosmology, z) else {
                continue;
            };
            let sigma_z = sigma(r.cmb.value.uncertainty);
            let uncertainty = if sigma_z.is_none() && r.kind == RedshiftKind::Photometric {
                FractionalUncertainty::NoneReported
            } else {
                let pv = (1.0 + z) * rules.peculiar_velocity_km_s / SPEED_OF_LIGHT;
                let total = sigma_z.unwrap_or(0.0).hypot(pv);
                reported(
                    slope * total / d,
                    sigma_z.map(|s| slope * s / d),
                    Some(slope * pv / d),
                )
            };
            out.push(Candidate {
                measurement: MeasurementRef::Redshift {
                    entry: at.clone(),
                    index,
                },
                rank: r.kind.into(),
                distance_mpc: d,
                uncertainty,
            });
        }
    }
    out
}

/// The candidate the rule prefers, by place in `candidates`, and what decided it.
fn winner(candidates: &[Candidate], rules: &RankingRules) -> Option<(usize, Decided)> {
    let by_rank = |a: &usize, b: &usize| {
        let (x, y) = (&candidates[*a], &candidates[*b]);
        let total = |c: &Candidate| c.uncertainty.total().unwrap_or(f64::INFINITY);
        rules
            .rank_of(x.rank)
            .cmp(&rules.rank_of(y.rank))
            .then(total(x).partial_cmp(&total(y)).unwrap_or(Ordering::Equal))
            .then(a.cmp(b))
    };
    let reported: Vec<usize> = (0..candidates.len())
        .filter(|&i| candidates[i].uncertainty.total().is_some())
        .collect();
    if reported.is_empty() {
        let best = (0..candidates.len()).min_by(by_rank)?;
        let decided = if candidates.len() == 1 {
            Decided::OnlyCandidate
        } else {
            Decided::MethodRankNoneReported
        };
        return Some((best, decided));
    }
    let total = |i: usize| candidates[i].uncertainty.total().unwrap_or(f64::INFINITY);
    let smallest = reported
        .iter()
        .map(|&i| total(i))
        .fold(f64::INFINITY, f64::min);
    let tied: Vec<usize> = reported
        .into_iter()
        .filter(|&i| total(i) <= smallest + rules.tie_tolerance)
        .collect();
    let best = tied.iter().copied().min_by(by_rank)?;
    let decided = if candidates.len() == 1 {
        Decided::OnlyCandidate
    } else if tied.len() > 1 {
        Decided::MethodRank
    } else {
        Decided::Uncertainty
    };
    Some((best, decided))
}

/// The preferred distance among `candidates`, with the best of the others as runner-up.
#[must_use]
pub fn choose(candidates: &[Candidate], rules: &RankingRules) -> Option<PreferredDistance> {
    let (best, decided) = winner(candidates, rules)?;
    let rest: Vec<Candidate> = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != best)
        .map(|(_, c)| c.clone())
        .collect();
    let runner_up = winner(&rest, rules).map(|(i, _)| {
        let c = &rest[i];
        (c.measurement.clone(), c.uncertainty)
    });
    let c = &candidates[best];
    Some(PreferredDistance {
        measurement: c.measurement.clone(),
        rank: c.rank,
        distance_mpc: c.distance_mpc,
        uncertainty: c.uncertainty,
        decided,
        runner_up,
    })
}
