//! The Galaxy model's sampling (build-the-galaxy-model).
#![expect(
    clippy::cast_precision_loss,
    reason = "counts of a few thousand points as the denominators of means"
)]

use super::*;
use crate::position::{Frame, between};

fn number(value: f64, unit: &str) -> Parameter {
    Parameter::cited(value, unit, "test", "Table 1")
}

fn colour() -> Colour {
    Colour {
        b_minus_v: None,
        srgb: [255, 255, 255],
        srgb_from: ParameterSource::Choice {
            reason: "test".to_owned(),
        },
    }
}

fn disc(code: u8, mass: f64, rd: f64, hz: f64, follows_arms: bool) -> Population {
    Population {
        code,
        id: format!("disc_{code}"),
        name: format!("Disc {code}"),
        density: Density::ExponentialDisc {
            scale_length: number(rd, "kiloparsec"),
            scale_height: number(hz, "kiloparsec"),
            max_radius: number(25.0, "kiloparsec"),
            follows_arms,
        },
        light: Light::Mass {
            mass: number(mass, "solar_mass"),
            mass_to_light: number(1.0, "1"),
        },
        colour: colour(),
    }
}

fn young(code: u8, share: f64, knots: Option<Knots>) -> Population {
    Population {
        code,
        id: format!("young_{code}"),
        name: format!("Young {code}"),
        density: Density::Arms {
            scale_height: number(0.05, "kiloparsec"),
            width_factor: number(1.0, "1"),
            radial_scale_length: number(2.6, "kiloparsec"),
            knots,
        },
        light: Light::Share {
            share: number(share, "1"),
        },
        colour: colour(),
    }
}

/// An arm along the line y = 6 kpc, x from -6 to 6, its width 0.3 kpc.
fn line_arm(id: &str, y: f64, extrapolated: bool) -> Fit {
    Fit {
        structure: id.to_owned(),
        extrapolated,
        path: (0..=120).map(|i| [f64::from(i) * 0.1 - 6.0, y]).collect(),
        width: Some(0.3),
    }
}

fn model(points: u64, populations: Vec<Population>) -> Model {
    Model {
        points,
        seed: 16,
        r0: number(8.15, "kiloparsec"),
        sun_height: number(0.0055, "kiloparsec"),
        populations,
        arms: ArmRules {
            order: vec![ArmOrder {
                arm: "test".to_owned(),
                structures: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            }],
            extrapolation_weight: Parameter::choice(0.45, "1", "test"),
            same_stretch: Parameter::choice(2.0, "kiloparsec", "test"),
            old_arm_contrast: Parameter::choice(1.0, "1", "test"),
            old_arm_spread: Parameter::choice(2.5, "1", "test"),
        },
        papers: vec![],
    }
}

/// The Galactocentric kpc of an ICRS parsec position under the model's placement.
fn galactocentric(p: Vec3) -> [f64; 3] {
    let g = between(Frame::Icrs, Frame::Galactic).apply(p.map(|c| c / 1000.0));
    [g[0] - 8.15, g[1], g[2] + 0.0055]
}

#[test]
fn the_same_model_and_seed_give_the_same_points_in_the_same_order() {
    let m = model(2000, vec![disc(1, 1.0, 2.6, 0.3, false)]);
    let a = sample(&m, &[]).unwrap();
    assert_eq!(a, sample(&m, &[]).unwrap());
    assert_eq!(a.len(), 2000);
    let other = Model { seed: 17, ..m };
    assert_ne!(a, sample(&other, &[]).unwrap());
}

#[test]
fn the_points_are_shared_by_light_and_add_up_exactly() {
    // Masses of 3 and 1 at a ratio of 1, and a share of a fifth: lights 3, 1 and 1.
    let m = model(
        1001,
        vec![
            disc(1, 3.0, 2.6, 0.3, false),
            disc(2, 1.0, 2.0, 0.9, false),
            young(3, 0.2, None),
        ],
    );
    let light = lights(&m).unwrap();
    assert!((light[2] - 1.0).abs() < 1e-12, "{light:?}");
    let counts = shares(m.points, &light);
    assert_eq!(counts.iter().sum::<u64>(), 1001);
    assert_eq!(counts, [601, 200, 200]);
    let none = model(10, vec![young(1, 0.5, None)]);
    assert_eq!(lights(&none), Err(ModelError::NoLight));
}

#[test]
fn an_exponential_disc_has_its_scale_length_and_height() {
    let disc_model = model(40_000, vec![disc(1, 1.0, 2.6, 0.3, false)]);
    let points = sample(&disc_model, &[]).unwrap();
    let count = points.len() as f64;
    let (mut radius, mut height) = (0.0, 0.0);
    for point in &points {
        let g = galactocentric(point.position);
        radius += g[0].hypot(g[1]);
        height += g[2].abs();
    }
    // A surface density exp(-R/Rd) has a mean radius of 2 Rd; a height exp(-|z|/h) a mean |z|
    // of h.
    assert!((radius / count - 5.2).abs() < 0.1, "{}", radius / count);
    assert!((height / count - 0.3).abs() < 0.01, "{}", height / count);
}

#[test]
fn the_galactic_centre_lies_8_15_kpc_toward_l_0_and_the_sun_above_the_plane() {
    let placement = Placement::new(8.15, 0.0055);
    let centre = between(Frame::Icrs, Frame::Galactic).apply(placement.to_icrs_pc([0.0; 3]));
    assert!((centre[0] - 8150.0).abs() < 1e-6, "{centre:?}");
    assert!(
        centre[1].abs() < 1e-6 && (centre[2] + 5.5).abs() < 1e-6,
        "{centre:?}"
    );
    assert_eq!(placement.in_plane([8.15, 1.0, 0.0]), [0.0, 1.0]);
}

#[test]
fn an_arm_population_lies_along_its_stretches_spread_by_the_arm_width() {
    let m = model(
        5000,
        vec![disc(1, 1.0, 2.6, 0.3, false), young(2, 0.5, None)],
    );
    let points = sample(&m, &[line_arm("a", 6.0, false)]).unwrap();
    let young: Vec<[f64; 3]> = points
        .iter()
        .filter(|p| p.population == 2)
        .map(|p| galactocentric(p.position))
        .collect();
    assert_eq!(young.len(), 2500);
    let n = young.len() as f64;
    let spread = (young.iter().map(|g| (g[1] - 6.0).powi(2)).sum::<f64>() / n).sqrt();
    assert!((spread - 0.3).abs() < 0.02, "{spread}");
    assert!(
        young
            .iter()
            .all(|g| g[0].abs() <= 6.0 + 1e-9 || (g[1] - 6.0).abs() > 0.0)
    );
    assert!(young.iter().all(|g| (g[1] - 6.0).abs() < 5.0 * 0.3 * 1.5));
}

#[test]
fn a_later_fit_near_an_earlier_one_is_left_out_and_an_extrapolation_follows_the_fits() {
    let rules = model(1, vec![]).arms;
    // b lies 1 kpc from a, within the 2 kpc of one stretch: left out; c, an extrapolation 5 kpc
    // away, is kept at its weight.
    let fits = [
        line_arm("b", 7.0, false),
        line_arm("a", 6.0, false),
        line_arm("c", 11.0, true),
    ];
    let s = stretches(&rules, &fits);
    let ids: Vec<&str> = s.iter().map(|s| s.structure.as_str()).collect();
    assert_eq!(ids, ["a", "c"]);
    assert_eq!(s[1].weight, 0.45);
    // A fit the rules do not order is not followed.
    assert_eq!(stretches(&rules, &[line_arm("z", 6.0, false)]), []);
}

#[test]
fn a_fit_that_continues_another_starts_where_it_stops_without_a_gap() {
    let rules = model(1, vec![]).arms;
    // a runs from x = -6 to 0; b, 0.3 kpc beside it, from -3 to 6: b's points alongside a are
    // left out, and b follows on from a's end.
    let mut a = line_arm("a", 6.0, false);
    a.path.retain(|p| p[0] <= 0.0);
    let mut b = line_arm("b", 6.3, false);
    b.path.retain(|p| p[0] >= -3.0);
    let s = stretches(&rules, &[a, b]);
    let ids: Vec<&str> = s.iter().map(|s| s.structure.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    let start = s[1].points[0][0];
    assert!(
        start > -0.05 && start < 0.15,
        "b starts at x = {start}, a ends at 0"
    );
}

#[test]
fn a_model_of_the_arms_without_an_arm_is_refused() {
    let m = model(10, vec![disc(1, 1.0, 2.6, 0.3, false), young(2, 0.5, None)]);
    assert_eq!(
        sample(&m, &[]),
        Err(ModelError::NoArms("young_2".to_owned()))
    );
}

#[test]
fn knots_gather_their_points_close_together() {
    let knots = Knots {
        per_knot: Parameter::choice(20.0, "1", "test"),
        knot_radius: Parameter::choice(0.02, "kiloparsec", "test"),
    };
    let m = model(
        2000,
        vec![disc(1, 1.0, 2.6, 0.3, false), young(2, 0.5, Some(knots))],
    );
    let points = sample(&m, &[line_arm("a", 6.0, false)]).unwrap();
    let knot: Vec<[f64; 3]> = points
        .iter()
        .filter(|p| p.population == 2)
        .take(20)
        .map(|p| galactocentric(p.position))
        .collect();
    let centre = [0, 1].map(|i| knot.iter().map(|g| g[i]).sum::<f64>() / knot.len() as f64);
    assert!(
        knot.iter()
            .all(|g| (g[0] - centre[0]).hypot(g[1] - centre[1]) < 0.1)
    );
}

#[test]
fn the_first_points_of_a_population_in_a_cube_are_covered_as_many_as_its_counterparts() {
    let mut counts = Counts::default();
    counts.add([10.0, 10.0, 10.0], Counterpart::Stars);
    counts.add([20.0, 20.0, 20.0], Counterpart::Stars);
    counts.add([300.0, 10.0, 10.0], Counterpart::YoungTracers);
    let points = [
        ([1.0, 1.0, 1.0], 1),
        ([2.0, 2.0, 2.0], 2),
        ([3.0, 3.0, 3.0], 1),
        ([4.0, 4.0, 4.0], 1),
        ([260.0, 1.0, 1.0], 2),
        ([270.0, 1.0, 1.0], 2),
    ];
    let side = |code: u8| {
        if code == 2 {
            Counterpart::YoungTracers
        } else {
            Counterpart::Stars
        }
    };
    let covered = covered(&points, side, &counts);
    assert_eq!(covered, [true, false, true, false, true, false]);
    assert_eq!(cube_of([-1.0, 249.0, 250.0]), [-1, 0, 1]);
}

#[test]
fn the_thin_disc_is_denser_along_the_arms_by_its_contrast() {
    let mut m = model(20_000, vec![disc(1, 1.0, 6.0, 0.3, true)]);
    m.arms.old_arm_contrast = Parameter::choice(3.0, "1", "test");
    m.arms.old_arm_spread = Parameter::choice(1.0, "1", "test");
    let points = sample(&m, &[line_arm("a", 6.0, false)]).unwrap();
    let band = |lo: f64, hi: f64| {
        points
            .iter()
            .map(|p| galactocentric(p.position))
            .filter(|g| g[0].abs() < 4.0 && (lo..hi).contains(&g[1]))
            .count() as f64
    };
    // On the arm, within 0.15 kpc of its line, against two bands as wide beside it, 1.5 kpc
    // away: about three times as dense.
    let on = band(5.85, 6.15);
    let off = f64::midpoint(band(4.35, 4.65), band(7.35, 7.65));
    assert!(on / off > 2.2 && on / off < 3.5, "{on} {off}");
}
