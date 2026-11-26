//! The definition file's numbers turned into univ-model's parameters, each in the unit the
//! model reads it in, with where it comes from.

use univ_model::galaxy_model::{
    Colour, Density, Knots, Light, LongBar, Parameter, ParameterSource,
};
use univ_model::units::{Dimension, Unit};
use univ_model::value::Uncertainty;

use super::{ColourFile, DEG, DensityFile, KPC, LightFile, Number, ONE};
use crate::structures::Locator;

/// The model's density of a population's `density`.
pub(super) fn density(density: &DensityFile) -> Result<Density, String> {
    let p = parameter;
    Ok(match density {
        DensityFile::ExponentialDisc {
            scale_length,
            scale_height,
            max_radius,
            follows_arms,
        } => Density::ExponentialDisc {
            scale_length: p(scale_length, KPC)?,
            scale_height: p(scale_height, KPC)?,
            max_radius: p(max_radius, KPC)?,
            follows_arms: *follows_arms,
        },
        DensityFile::Bar {
            angle,
            bulge_scales: [along, across, up],
            long_bar,
            long_bar_share,
        } => Density::Bar {
            angle: p(angle, DEG)?,
            bulge_scales: [p(along, KPC)?, p(across, KPC)?, p(up, KPC)?],
            long_bar: Box::new(LongBar {
                angle: p(&long_bar.angle, DEG)?,
                x0: p(&long_bar.x0, KPC)?,
                y0: p(&long_bar.y0, KPC)?,
                z0: p(&long_bar.z0, KPC)?,
                c_perp: p(&long_bar.c_perp, ONE)?,
                r_out: p(&long_bar.r_out, KPC)?,
                sigma_out: p(&long_bar.sigma_out, KPC)?,
                r_in: p(&long_bar.r_in, KPC)?,
                sigma_in: p(&long_bar.sigma_in, KPC)?,
            }),
            long_bar_share: p(long_bar_share, ONE)?,
        },
        DensityFile::Halo {
            inner_slope,
            outer_slope,
            break_radius,
            flattening,
            min_radius,
            max_radius,
        } => Density::Halo {
            inner_slope: p(inner_slope, ONE)?,
            outer_slope: p(outer_slope, ONE)?,
            break_radius: p(break_radius, KPC)?,
            flattening: p(flattening, ONE)?,
            min_radius: p(min_radius, KPC)?,
            max_radius: p(max_radius, KPC)?,
        },
        DensityFile::Arms {
            scale_height,
            width_factor,
            radial_scale_length,
            knots,
        } => Density::Arms {
            scale_height: p(scale_height, KPC)?,
            width_factor: p(width_factor, ONE)?,
            radial_scale_length: p(radial_scale_length, KPC)?,
            knots: knots
                .as_ref()
                .map(|k| -> Result<Knots, String> {
                    Ok(Knots {
                        per_knot: p(&k.per_knot, ONE)?,
                        knot_radius: p(&k.knot_radius, KPC)?,
                    })
                })
                .transpose()?,
        },
    })
}

/// The model's light of a population's `light`.
pub(super) fn light(light: &LightFile) -> Result<Light, String> {
    Ok(match light {
        LightFile::ByMass {
            mass,
            mass_to_light,
        } => Light::Mass {
            mass: parameter(mass, Unit::SolarMass)?,
            mass_to_light: parameter(mass_to_light, Unit::SolarMassPerSolarLuminosity)?,
        },
        LightFile::Share(share) => Light::Share {
            share: parameter(share, ONE)?,
        },
    })
}

/// The model's colour of a population's `colour`: derived from its colour index, or chosen.
pub(super) fn colour(colour: &ColourFile) -> Result<Colour, String> {
    Ok(Colour {
        b_minus_v: colour
            .b_minus_v
            .as_ref()
            .map(|b| parameter(b, Unit::Magnitude))
            .transpose()?,
        srgb: colour.srgb,
        srgb_from: match (&colour.derived, &colour.choice) {
            (_, Some(reason)) => ParameterSource::Choice {
                reason: reason.clone(),
            },
            (how, None) => ParameterSource::Derived {
                how: how.clone().unwrap_or_default(),
            },
        },
    })
}

/// The name a model parameter gives `unit`.
fn unit_name(unit: Unit) -> String {
    match unit.dimension() {
        Dimension::Dimensionless => "1".to_owned(),
        _ => serde_json::to_value(unit)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
    }
}

/// `number` as a model parameter in `unit`, with where it comes from.
pub(super) fn parameter(number: &Number, unit: Unit) -> Result<Parameter, String> {
    let factor = number.unit().factor_to(unit).map_err(|e| e.to_string())?;
    let (kind, uncertainty, source) = match number {
        Number::Cited(c) => (
            Some(c.kind),
            c.quantity.uncertainty.scaled(factor),
            ParameterSource::Cited {
                paper: c.cite.paper.clone(),
                locator: locator_text(&c.cite.locator),
            },
        ),
        Number::Choice(c) => (
            None,
            Uncertainty::NoneReported,
            ParameterSource::Choice {
                reason: c.choice.clone(),
            },
        ),
    };
    Ok(Parameter {
        value: number.value() * factor,
        unit: unit_name(unit),
        kind,
        uncertainty,
        source,
    })
}

/// Where in its paper a number was read, in words.
fn locator_text(locator: &Locator) -> String {
    match locator {
        Locator::Sentence { quote, .. } => format!("\"{quote}\""),
        Locator::Table {
            number,
            row,
            column,
            cell,
        } => {
            let cell = cell
                .as_ref()
                .map_or_else(String::new, |c| format!(", \"{c}\""));
            format!("Table {number}, row {row}, column {column}{cell}")
        }
        Locator::Figure { number, how } => format!("Figure {number}: {how}"),
    }
}
