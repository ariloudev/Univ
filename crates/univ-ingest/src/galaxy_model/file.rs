//! The Galaxy model's definition file as written: `rules/galaxy-model.yaml`.

use serde::Deserialize;
use serde::de::{self, Deserializer};
use univ_model::units::Unit;

use crate::structures::{Arm, Cited, Paper};

/// A number of the definition: cited, as a structure file cites its numbers, or this project's
/// choice, with its reason.
#[derive(Clone, Debug, PartialEq)]
pub enum Number {
    /// Boxed: a citation is many times a choice's size.
    Cited(Box<Cited>),
    Choice(Choice),
}

/// A number no paper gives, which this project chose, and why.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub value: f64,
    pub unit: Unit,
    /// Why this project chose it.
    pub choice: String,
}

impl Number {
    /// Its value, in its own unit.
    #[must_use]
    pub fn value(&self) -> f64 {
        match self {
            Self::Cited(c) => c.value(),
            Self::Choice(c) => c.value,
        }
    }

    /// Its unit.
    #[must_use]
    pub fn unit(&self) -> Unit {
        match self {
            Self::Cited(c) => c.unit(),
            Self::Choice(c) => c.unit,
        }
    }
}

/// The refusal of a number that neither cites a paper nor is marked as this project's choice.
fn unmarked(value: &serde_norway::Value) -> String {
    let shown = match value {
        serde_norway::Value::Mapping(m) => m
            .get("value")
            .and_then(serde_norway::Value::as_f64)
            .map(|v| v.to_string()),
        other => other.as_f64().map(|v| v.to_string()),
    }
    .unwrap_or_else(|| "given".to_owned());
    format!(
        "the number {shown} neither cites a paper nor is marked as this project's choice: give \
         it a cite naming the paper and the sentence, table or figure, or a choice with its reason"
    )
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_norway::Value::deserialize(deserializer)?;
        let has = |key: &str| {
            value
                .as_mapping()
                .is_some_and(|m| m.contains_key(serde_norway::Value::from(key)))
        };
        if has("choice") {
            serde_norway::from_value(value)
                .map(Self::Choice)
                .map_err(de::Error::custom)
        } else if has("cite") {
            Cited::deserialize(value)
                .map(|c| Self::Cited(Box::new(c)))
                .map_err(de::Error::custom)
        } else {
            Err(de::Error::custom(unmarked(&value)))
        }
    }
}

/// The definition file.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    /// How many model points the model is sampled into.
    pub points: u64,
    /// Where that number comes from.
    pub points_basis: String,
    pub seed: u64,
    pub papers: Vec<Paper>,
    pub r0: Number,
    pub sun_height: Number,
    pub populations: Vec<PopulationFile>,
    pub arms: ArmsFile,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationFile {
    pub code: u8,
    pub id: String,
    pub name: String,
    #[serde(with = "serde_norway::with::singleton_map")]
    pub density: DensityFile,
    #[serde(with = "serde_norway::with::singleton_map")]
    pub light: LightFile,
    pub colour: ColourFile,
}

/// A population's density, written as its form's name over its numbers, so that a refusal names
/// the number's whole path.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DensityFile {
    ExponentialDisc {
        scale_length: Number,
        scale_height: Number,
        max_radius: Number,
        follows_arms: bool,
    },
    Bar {
        angle: Number,
        bulge_scales: [Number; 3],
        long_bar: Box<LongBarFile>,
        long_bar_share: Number,
    },
    Halo {
        inner_slope: Number,
        outer_slope: Number,
        break_radius: Number,
        flattening: Number,
        min_radius: Number,
        max_radius: Number,
    },
    Arms {
        scale_height: Number,
        width_factor: Number,
        radial_scale_length: Number,
        #[serde(default)]
        knots: Option<KnotsFile>,
    },
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LongBarFile {
    pub angle: Number,
    pub x0: Number,
    pub y0: Number,
    pub z0: Number,
    pub c_perp: Number,
    pub r_out: Number,
    pub sigma_out: Number,
    pub r_in: Number,
    pub sigma_in: Number,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnotsFile {
    pub per_knot: Number,
    pub knot_radius: Number,
}

/// A population's light: `by_mass`, its mass and mass-to-light ratio, or its `share` of the
/// Galaxy's light.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LightFile {
    ByMass { mass: Number, mass_to_light: Number },
    Share(Number),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColourFile {
    #[serde(default)]
    pub b_minus_v: Option<Number>,
    pub srgb: [u8; 3],
    /// How the sRGB comes from the colour index, where it does.
    #[serde(default)]
    pub derived: Option<String>,
    /// Why this project chose the sRGB, where it did.
    #[serde(default)]
    pub choice: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmsFile {
    pub order: Vec<ArmOrderFile>,
    pub extrapolation_weight: Number,
    pub same_stretch: Number,
    pub old_arm_contrast: Number,
    pub old_arm_spread: Number,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmOrderFile {
    pub arm: Arm,
    pub structures: Vec<String>,
}
