//! Drawing a point of each population of the Galaxy model from its density, in Galactocentric
//! kpc: x from the Galactic Centre away from the Sun, y toward l = 90° at the Sun (the direction
//! of rotation), z toward the north Galactic pole from the plane.

use super::arms::{ArmSampler, Nearness};
use super::model::{Density, LongBar};
use super::random::Random;

/// What drawing a population's points needs beside its density: the arms' segments and their
/// nearness, and the arm rules' contrast of the old disc.
pub struct Context<'a> {
    pub arms: &'a ArmSampler<'a>,
    pub nearness: &'a Nearness,
    pub old_arm_contrast: f64,
}

/// A point drawn from `density`, Galactocentric kpc; `None` where the density cannot be drawn
/// from, such as an arm population when no arm has a stretch.
pub fn draw(density: &Density, context: &Context<'_>, random: &mut Random) -> Option<[f64; 3]> {
    match density {
        Density::ExponentialDisc {
            scale_length,
            scale_height,
            max_radius,
            follows_arms,
        } => {
            let contrast = if *follows_arms {
                context.old_arm_contrast.max(1.0)
            } else {
                1.0
            };
            Some(disc(
                random,
                scale_length.value,
                scale_height.value,
                max_radius.value,
                |p| (1.0 + (contrast - 1.0) * context.nearness.at(p)) / contrast,
            ))
        }
        Density::Bar {
            angle,
            bulge_scales,
            long_bar,
            long_bar_share,
        } => Some(if random.uniform() < long_bar_share.value {
            long_bar_point(random, long_bar)
        } else {
            let [a, b, c] = bulge_scales.clone().map(|p| p.value);
            rotated(
                [random.laplace(a), random.laplace(b), random.laplace(c)],
                angle.value,
            )
        }),
        Density::Halo {
            inner_slope,
            outer_slope,
            break_radius,
            flattening,
            min_radius,
            max_radius,
        } => Some(halo(
            random,
            [inner_slope.value, outer_slope.value],
            break_radius.value,
            flattening.value,
            [min_radius.value, max_radius.value],
        )),
        Density::Arms {
            scale_height,
            width_factor,
            ..
        } => {
            if context.arms.is_empty() {
                return None;
            }
            let [x, y] = context.arms.draw(random, width_factor.value);
            Some([x, y, random.laplace(scale_height.value)])
        }
    }
}

/// A point of an exponential disc of scale length `rd` and height `hz` within `r_max`: a radius
/// of the surface density `exp(-R/rd)`, a Gamma(2, rd) draw, a uniform azimuth and a Laplace
/// height, kept with probability `keep` of its place, as the thin disc's rise along the arms.
fn disc(
    random: &mut Random,
    rd: f64,
    hz: f64,
    r_max: f64,
    keep: impl Fn([f64; 2]) -> f64,
) -> [f64; 3] {
    loop {
        let r = random.exponential(rd) + random.exponential(rd);
        if r > r_max {
            continue;
        }
        let phi = std::f64::consts::TAU * random.uniform();
        let p = [r * phi.cos(), r * phi.sin()];
        if random.uniform() < keep(p) {
            return [p[0], p[1], random.laplace(hz)];
        }
    }
}

/// `p` turned by `angle_deg` in the plane from the Sun–Galactic Centre line toward positive
/// longitude: the bar's near end lies at l > 0, its long axis at `angle_deg` from the line to
/// the Sun.
fn rotated(p: [f64; 3], angle_deg: f64) -> [f64; 3] {
    // The bar's long axis points toward the Sun's side (−x) turned toward +y.
    let a = angle_deg.to_radians();
    let (along, across) = (p[0], p[1]);
    [
        -along * a.cos() - across * a.sin(),
        along * a.sin() - across * a.cos(),
        p[2],
    ]
}

/// A point of the long bar of Wegg, Gerhard & Portail 2015: drawn uniformly in its box and kept
/// by its density, `exp(-[(|x|/x0)^c + (|y|/y0)^c]^(1/c))`, cut beyond `r_out` and within
/// `r_in` by `exp(-((R - r_out)/σ_out)²)` and `exp(-((r_in - R)/σ_in)²)` where those are
/// positive; exponential in height.
fn long_bar_point(random: &mut Random, bar: &LongBar) -> [f64; 3] {
    let (x0, y0, c) = (bar.x0.value, bar.y0.value, bar.c_perp.value);
    let (r_out, s_out, r_in, s_in) = (
        bar.r_out.value,
        bar.sigma_out.value,
        bar.r_in.value,
        bar.sigma_in.value,
    );
    let extent = r_out + 3.0 * s_out;
    let cut = |x: f64| if x > 0.0 { (-x * x).exp() } else { 1.0 };
    loop {
        let x = (2.0 * random.uniform() - 1.0) * extent;
        let y = (2.0 * random.uniform() - 1.0) * extent;
        let r = x.hypot(y);
        let inplane = ((x.abs() / x0).powf(c) + (y.abs() / y0).powf(c)).powf(1.0 / c);
        let density = (-inplane).exp() * cut((r - r_out) / s_out) * cut((r_in - r) / s_in);
        if random.uniform() < density {
            return rotated([x, y, random.laplace(bar.z0.value)], bar.angle.value);
        }
    }
}

/// A point of a flattened double power law: density `m^slope`, `m` the ellipsoidal radius
/// `sqrt(R² + (z/q)²)`, the inner slope within the break and the outer beyond it, between the
/// two radii; drawn by `m` from its mass profile, then a direction, z squeezed by `q`.
fn halo(
    random: &mut Random,
    [inner, outer]: [f64; 2],
    break_radius: f64,
    q: f64,
    [m_min, m_max]: [f64; 2],
) -> [f64; 3] {
    // The mass within a shell of ellipsoidal radius m grows as m^(slope + 3) dm-integrated; draw
    // m piecewise from the two power laws by their masses, continuous at the break.
    let shell = |slope: f64, a: f64, b: f64, scale: f64| {
        let k = slope + 3.0;
        scale * (b.powf(k) - a.powf(k)) / k
    };
    let inner_mass = shell(inner, m_min, break_radius, 1.0);
    let outer_scale = break_radius.powf(inner - outer);
    let outer_mass = shell(outer, break_radius, m_max, outer_scale);
    let draw_m = |random: &mut Random, slope: f64, a: f64, b: f64| {
        let k = slope + 3.0;
        let u = random.uniform();
        (a.powf(k) + u * (b.powf(k) - a.powf(k))).powf(1.0 / k)
    };
    let m = if random.uniform() * (inner_mass + outer_mass) < inner_mass {
        draw_m(random, inner, m_min, break_radius)
    } else {
        draw_m(random, outer, break_radius, m_max)
    };
    let cos_theta = 2.0 * random.uniform() - 1.0;
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
    let phi = std::f64::consts::TAU * random.uniform();
    [
        m * sin_theta * phi.cos(),
        m * sin_theta * phi.sin(),
        m * q * cos_theta,
    ]
}
