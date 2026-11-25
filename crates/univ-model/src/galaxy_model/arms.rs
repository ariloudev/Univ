//! The stretches of the spiral arms the arms' populations follow (build-the-galaxy-model).
//!
//! Each arm has fits from several papers, preferred in the order the model's arm rules give
//! them; each fit is a fitted range or its extrapolation. Two fits of one arm can lie on the same
//! stretch of it, a little apart where their papers disagree, or on different stretches at the
//! same azimuth, a winding apart. So a stretch is resolved by place, not by azimuth: the fitted
//! ranges of an arm first, in order, then its extrapolations, in order, each keeping only its
//! points that lie farther than the arm rules' `same_stretch` from every point already kept.

use super::model::ArmRules;
use super::random::Random;

/// A fit of an arm as the model takes it: its structure, whether it is an extrapolation, its
/// path in the plane, Galactocentric kpc, and its width, kpc, where its paper gives one.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub structure: String,
    pub extrapolated: bool,
    pub path: Vec<[f64; 2]>,
    pub width: Option<f64>,
}

/// A stretch of an arm the arms' populations follow: its points, its width and its weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Stretch {
    /// The structure it comes from.
    pub structure: String,
    pub points: Vec<[f64; 2]>,
    pub width: f64,
    /// 1 for a fitted range, the arm rules' extrapolation weight for an extrapolation.
    pub weight: f64,
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The stretches of every arm the rules order, from `fits` by structure id: each fit's runs of
/// points that lie farther than `same_stretch` from the points of the arm's fits preferred
/// before it. A fit without a width takes the width of the arm's first fit that has one; an arm
/// whose fits give none takes no stretch.
#[must_use]
pub fn stretches(rules: &ArmRules, fits: &[Fit]) -> Vec<Stretch> {
    let same = rules.same_stretch.value;
    let mut out = Vec::new();
    for order in &rules.order {
        let of_arm: Vec<&Fit> = order
            .structures
            .iter()
            .filter_map(|id| fits.iter().find(|f| &f.structure == id))
            .collect();
        let Some(arm_width) = of_arm.iter().find_map(|f| f.width) else {
            continue;
        };
        let ordered = of_arm
            .iter()
            .filter(|f| !f.extrapolated)
            .chain(of_arm.iter().filter(|f| f.extrapolated));
        let mut kept: Vec<[f64; 2]> = Vec::new();
        for fit in ordered {
            let free: Vec<bool> = fit
                .path
                .iter()
                .map(|p| kept.iter().all(|k| distance(*p, *k) > same))
                .collect();
            let mut run: Vec<[f64; 2]> = Vec::new();
            let flush = |run: &mut Vec<[f64; 2]>, out: &mut Vec<Stretch>| {
                if run.len() >= 2 {
                    out.push(Stretch {
                        structure: fit.structure.clone(),
                        points: std::mem::take(run),
                        width: fit.width.unwrap_or(arm_width),
                        weight: if fit.extrapolated {
                            rules.extrapolation_weight.value
                        } else {
                            1.0
                        },
                    });
                }
                run.clear();
            };
            for (p, free) in fit.path.iter().zip(&free) {
                if *free {
                    run.push(*p);
                } else {
                    flush(&mut run, &mut out);
                }
            }
            flush(&mut run, &mut out);
            kept.extend(
                fit.path
                    .iter()
                    .zip(&free)
                    .filter(|(_, f)| **f)
                    .map(|(p, _)| *p),
            );
        }
    }
    out
}

/// The segments of `stretches`, each with its share of the arms' points: its length times its
/// stretch's weight times the falloff `exp(-R / radial_scale)` at its middle, `R` its
/// Galactocentric radius, kpc. Points are drawn along them by [`ArmSampler::draw`].
pub struct ArmSampler<'a> {
    stretches: &'a [Stretch],
    /// Each segment as (stretch, first point), with the running total of the weights.
    segments: Vec<(usize, usize)>,
    cumulative: Vec<f64>,
}

impl<'a> ArmSampler<'a> {
    #[must_use]
    pub fn new(stretches: &'a [Stretch], radial_scale: f64) -> Self {
        let mut segments = Vec::new();
        let mut cumulative = Vec::new();
        let mut total = 0.0;
        for (s, stretch) in stretches.iter().enumerate() {
            for (i, pair) in stretch.points.windows(2).enumerate() {
                let middle = [
                    f64::midpoint(pair[0][0], pair[1][0]),
                    f64::midpoint(pair[0][1], pair[1][1]),
                ];
                let r = middle[0].hypot(middle[1]);
                total += distance(pair[0], pair[1]) * stretch.weight * (-r / radial_scale).exp();
                segments.push((s, i));
                cumulative.push(total);
            }
        }
        Self {
            stretches,
            segments,
            cumulative,
        }
    }

    /// Whether there is a segment to draw from.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cumulative.last().is_none_or(|t| *t <= 0.0)
    }

    /// A point on the arms: on a segment drawn by its share, at a uniform place along it, moved
    /// across it by a Gaussian of its stretch's width times `width_factor`; in the plane, kpc.
    pub fn draw(&self, random: &mut Random, width_factor: f64) -> [f64; 2] {
        let total = self.cumulative.last().copied().unwrap_or_default();
        let target = random.uniform() * total;
        let segment = self
            .cumulative
            .partition_point(|c| *c < target)
            .min(self.segments.len().saturating_sub(1));
        let (which, first) = self.segments[segment];
        let stretch = &self.stretches[which];
        let (start, end) = (stretch.points[first], stretch.points[first + 1]);
        let along = random.uniform();
        let on = [
            start[0] + along * (end[0] - start[0]),
            start[1] + along * (end[1] - start[1]),
        ];
        let length = distance(start, end).max(f64::MIN_POSITIVE);
        let normal = [-(end[1] - start[1]) / length, (end[0] - start[0]) / length];
        let across = random.normal() * stretch.width * width_factor;
        [on[0] + normal[0] * across, on[1] + normal[1] * across]
    }
}

/// How near the arms a place in the plane lies, 0 to 1: a raster of the largest Gaussian
/// `exp(-d² / 2σ²)` of any stretch point, `σ` its stretch's width times `spread`, over the
/// disc, in cells of [`NEARNESS_CELL`] kpc out to [`NEARNESS_EXTENT`] kpc from the centre.
pub struct Nearness {
    cells: Vec<f32>,
    side: usize,
}

/// The cell of the arms' nearness raster, kpc.
pub const NEARNESS_CELL: f64 = 0.05;
/// How far from the Galactic Centre the raster reaches along each axis, kpc.
pub const NEARNESS_EXTENT: f64 = 26.0;

impl Nearness {
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "cell indices of a raster of at most a few thousand cells a side"
    )]
    pub fn new(stretches: &[Stretch], spread: f64) -> Self {
        let side = (2.0 * NEARNESS_EXTENT / NEARNESS_CELL).ceil() as usize;
        let last = i64::try_from(side).unwrap_or(i64::MAX);
        let mut cells = vec![0.0_f32; side * side];
        for stretch in stretches {
            let sigma = (stretch.width * spread).max(NEARNESS_CELL);
            let reach = (3.0 * sigma / NEARNESS_CELL).ceil() as i64;
            for p in &stretch.points {
                let (ci, cj) = (cell_of(p[0]), cell_of(p[1]));
                for di in -reach..=reach {
                    for dj in -reach..=reach {
                        let (i, j) = (ci + di, cj + dj);
                        if i < 0 || j < 0 || i >= last || j >= last {
                            continue;
                        }
                        let centre = |k: i64| (k as f64 + 0.5) * NEARNESS_CELL - NEARNESS_EXTENT;
                        let d = distance([centre(i), centre(j)], *p);
                        let g = (-(d * d) / (2.0 * sigma * sigma)).exp() as f32;
                        let cell = &mut cells[i as usize * side + j as usize];
                        *cell = cell.max(g);
                    }
                }
            }
        }
        Self { cells, side }
    }

    /// The nearness of the place `p`, kpc in the plane: 0 beyond the raster.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to lie within the raster"
    )]
    pub fn at(&self, p: [f64; 2]) -> f64 {
        let (i, j) = (cell_of(p[0]), cell_of(p[1]));
        let side = i64::try_from(self.side).unwrap_or(i64::MAX);
        if i < 0 || j < 0 || i >= side || j >= side {
            return 0.0;
        }
        f64::from(self.cells[i as usize * self.side + j as usize])
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a cell index within a few thousand"
)]
fn cell_of(x: f64) -> i64 {
    ((x + NEARNESS_EXTENT) / NEARNESS_CELL).floor() as i64
}
