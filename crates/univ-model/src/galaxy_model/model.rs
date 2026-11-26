//! The Galaxy model's definition as a release publishes it: its populations, their densities,
//! their light and colours, how they follow the spiral arms, and where each number comes from.

use serde::{Deserialize, Serialize};

use crate::value::{KindOfValue, Uncertainty};

/// A number of the model: its value in the unit the model reads it in, its uncertainty, and
/// either the paper it comes from or the reason this project chose it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Parameter {
    pub value: f64,
    /// The unit `value` is in, such as `kiloparsec`, `degree`, `solar_mass` or `1` for a ratio.
    pub unit: String,
    /// Its kind of value, where a paper gives it; none for this project's choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<KindOfValue>,
    /// Its uncertainty in `unit`, as its paper gives it; none reported for a number given
    /// without one and for this project's choice, never zero. A definition written before the
    /// model kept uncertainties reads as none reported.
    #[serde(default = "none_reported")]
    pub uncertainty: Uncertainty,
    pub source: ParameterSource,
}

fn none_reported() -> Uncertainty {
    Uncertainty::NoneReported
}

/// Where a number of the model comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "from")]
pub enum ParameterSource {
    /// A paper of the model, by its id, and where in it the number was read.
    Cited { paper: String, locator: String },
    /// This project's choice, which no paper makes, and why.
    Choice { reason: String },
    /// Derived from other numbers of the model, and how.
    Derived { how: String },
}

impl Parameter {
    /// A cited number, for tests and fixtures.
    #[must_use]
    pub fn cited(value: f64, unit: &str, paper: &str, locator: &str) -> Self {
        Self {
            value,
            unit: unit.to_owned(),
            kind: Some(KindOfValue::Estimated),
            uncertainty: Uncertainty::NoneReported,
            source: ParameterSource::Cited {
                paper: paper.to_owned(),
                locator: locator.to_owned(),
            },
        }
    }

    /// A number this project chose, for tests and fixtures.
    #[must_use]
    pub fn choice(value: f64, unit: &str, reason: &str) -> Self {
        Self {
            value,
            unit: unit.to_owned(),
            kind: None,
            uncertainty: Uncertainty::NoneReported,
            source: ParameterSource::Choice {
                reason: reason.to_owned(),
            },
        }
    }
}

/// A paper the model cites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelPaper {
    pub id: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
}

/// The Galaxy model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Model {
    /// How many model points the model is sampled into.
    pub points: u64,
    /// The seed of the sampling.
    pub seed: u64,
    /// Where the Sun stands: the Galactic Centre's distance, kpc, and the Sun's height above the
    /// Galactic plane, kpc.
    pub r0: Parameter,
    pub sun_height: Parameter,
    pub populations: Vec<Population>,
    pub arms: ArmRules,
    pub papers: Vec<ModelPaper>,
}

/// A population of the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Population {
    /// The code its model points carry, from 1.
    pub code: u8,
    /// Its id, such as `thin_disc`.
    pub id: String,
    pub name: String,
    pub density: Density,
    pub light: Light,
    pub colour: Colour,
}

/// How a population's stars are laid out, in kpc and degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Density {
    /// An exponential disc: exponential in Galactocentric radius and in height, out to
    /// `max_radius`; the thin disc's density rises along the arms by the arm rules' contrast.
    ExponentialDisc {
        scale_length: Parameter,
        scale_height: Parameter,
        max_radius: Parameter,
        follows_arms: bool,
    },
    /// The bar and bulge: a triaxial exponential bulge and a long bar, both along the bar's
    /// angle from the Sun–Galactic Centre line, the long bar taking `long_bar_share` of the
    /// population's points.
    Bar {
        angle: Parameter,
        bulge_scales: [Parameter; 3],
        long_bar: Box<LongBar>,
        long_bar_share: Parameter,
    },
    /// A flattened double power law in the ellipsoidal radius, between two radii.
    Halo {
        inner_slope: Parameter,
        outer_slope: Parameter,
        break_radius: Parameter,
        flattening: Parameter,
        min_radius: Parameter,
        max_radius: Parameter,
    },
    /// Along the spiral arms' stretches, spread across each by its width times `width_factor`,
    /// exponential in height; knots gather `per_knot` points within `knot_radius`.
    Arms {
        scale_height: Parameter,
        width_factor: Parameter,
        radial_scale_length: Parameter,
        knots: Option<Knots>,
    },
}

/// The long bar of Wegg, Gerhard & Portail 2015, their equation for the thin bar: a boxy
/// exponential in the plane, exponential in height, cut off within and beyond its radii.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LongBar {
    pub angle: Parameter,
    pub x0: Parameter,
    pub y0: Parameter,
    pub z0: Parameter,
    pub c_perp: Parameter,
    pub r_out: Parameter,
    pub sigma_out: Parameter,
    pub r_in: Parameter,
    pub sigma_in: Parameter,
}

/// How a population of the arms gathers into knots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Knots {
    pub per_knot: Parameter,
    pub knot_radius: Parameter,
}

/// A population's light in the V band: from its stellar mass and mass-to-light ratio, or as a
/// share of the Galaxy's light.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "from")]
pub enum Light {
    Mass {
        mass: Parameter,
        mass_to_light: Parameter,
    },
    Share {
        share: Parameter,
    },
}

/// A population's display colour: sRGB, from its colour index or this project's choice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Colour {
    /// Its B − V colour index, where a paper gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_minus_v: Option<Parameter>,
    /// Its colour in sRGB, 0 to 255.
    pub srgb: [u8; 3],
    /// How the sRGB was had: derived from the colour index, or chosen and why.
    pub srgb_from: ParameterSource,
}

/// How the arms' populations follow the arms' fits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArmRules {
    /// The fits of each arm, by structure id, in the order the model prefers them.
    pub order: Vec<ArmOrder>,
    /// The weight of an extrapolation's points against a fitted range's.
    pub extrapolation_weight: Parameter,
    /// How near, kpc, a later fit of an arm may come to an earlier one before it is taken as the
    /// same stretch and left out there.
    pub same_stretch: Parameter,
    /// How much denser the thin disc is on an arm's path than between the arms.
    pub old_arm_contrast: Parameter,
    /// The old disc's spread across an arm, as a multiple of the arm's width.
    pub old_arm_spread: Parameter,
}

/// The fits of one arm, preferred first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArmOrder {
    pub arm: String,
    pub structures: Vec<String>,
}
