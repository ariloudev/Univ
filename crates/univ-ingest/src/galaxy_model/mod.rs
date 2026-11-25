//! The Galaxy model's definition, `rules/galaxy-model.yaml` (build-the-galaxy-model): read with
//! the checks every cited file takes, each number cited to its paper or marked as this project's
//! choice with its reason, and turned into univ-model's [`Model`], which a release samples and
//! publishes.

mod file;

use std::collections::BTreeSet;
use std::path::Path;

use univ_model::galaxy_model::{
    ArmOrder, ArmRules, Colour, Density, Knots, Light, LongBar, Model, ModelPaper, Parameter,
    ParameterSource, Population,
};
use univ_model::units::{Dimension, Unit};

pub use file::{
    ArmOrderFile, ArmsFile, Choice, ColourFile, DensityFile, KnotsFile, LightFile, LongBarFile,
    ModelFile, Number, PopulationFile,
};

use crate::structures::{CitedFile, Evidence, Locator, Paper, Problem, problem};
use crate::yaml::{FileError, read_checked};

/// The name of the definition among the release's rules.
pub const RULES_NAME: &str = "galaxy-model";

/// A number of the definition with its field and the unit the model reads it in.
type Field<'a> = (String, &'a Number, Unit);

const KPC: Unit = Unit::Kiloparsec;
const DEG: Unit = Unit::Degree;
const ONE: Unit = Unit::Dimensionless;

impl CitedFile for ModelFile {
    fn papers(&self) -> &[Paper] {
        &self.papers
    }
}

impl ModelFile {
    /// Reads and checks the definition in `file`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse, a number that neither
    /// cites a paper nor is marked as this project's choice, and the checks of
    /// [`ModelFile::check`].
    pub fn read(file: &Path) -> Result<Self, FileError> {
        read_checked(file, Self::check)
    }

    /// Every number of the definition, with its field and the unit the model reads it in.
    fn fields(&self) -> Vec<Field<'_>> {
        let arms = &self.arms;
        let mut out: Vec<Field<'_>> = vec![
            ("r0".to_owned(), &self.r0, KPC),
            ("sun_height".to_owned(), &self.sun_height, KPC),
        ];
        for (name, n, unit) in [
            ("extrapolation_weight", &arms.extrapolation_weight, ONE),
            ("same_stretch", &arms.same_stretch, KPC),
            ("old_arm_contrast", &arms.old_arm_contrast, ONE),
            ("old_arm_spread", &arms.old_arm_spread, ONE),
        ] {
            out.push((format!("arms.{name}"), n, unit));
        }
        for (i, p) in self.populations.iter().enumerate() {
            let numbers = density_numbers(&p.density)
                .into_iter()
                .map(|(name, n, unit)| (format!("density.{name}"), n, unit))
                .chain(light_numbers(&p.light))
                .chain(
                    p.colour
                        .b_minus_v
                        .iter()
                        .map(|b| ("colour.b_minus_v".to_owned(), b, Unit::Magnitude)),
                );
            out.extend(
                numbers.map(|(name, n, unit)| (format!("populations[{i}].{name}"), n, unit)),
            );
        }
        out
    }

    /// [`ModelFile::check`], as a rules file's checks are given.
    ///
    /// # Errors
    ///
    /// The field and the problem of the first check that fails.
    pub fn checks(&self) -> Result<(), (String, String)> {
        self.check()
    }

    /// The checks serde cannot make: the papers stand and every citation names one of them and
    /// holds its number; every choice gives its reason; every number is in a unit of the
    /// dimension the model reads it in; the populations' codes and ids are distinct, from 1;
    /// each colour is derived from its colour index or chosen; and every number of the free
    /// text is cited.
    fn check(&self) -> Result<(), Problem> {
        self.check_papers()?;
        if self.points == 0 {
            return problem("points", "is not positive");
        }
        for (name, text) in [("points_basis", &self.points_basis), ("notes", &self.notes)] {
            if text.trim().is_empty() {
                return problem(name, "is empty");
            }
        }
        for (field, number, unit) in self.fields() {
            if number.unit().dimension() != unit.dimension() {
                return problem(
                    &format!("{field}.unit"),
                    format!("is no unit of {:?}", unit.dimension()),
                );
            }
            match number {
                Number::Cited(c) => {
                    self.check_citation(&field, &c.cite, Some(&**c))?;
                    self.check_value(&field, c)?;
                }
                Number::Choice(c) if c.choice.trim().is_empty() => {
                    return problem(&format!("{field}.choice"), "gives no reason");
                }
                Number::Choice(_) => {}
            }
        }
        let mut codes = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (i, p) in self.populations.iter().enumerate() {
            if p.code == 0 || !codes.insert(p.code) || !ids.insert(p.id.as_str()) {
                return problem(
                    &format!("populations[{i}]"),
                    "a population's code, from 1, and its id are its own",
                );
            }
            let c = &p.colour;
            let derived = c.derived.is_some() && c.b_minus_v.is_some();
            if derived == c.choice.is_some() {
                return problem(
                    &format!("populations[{i}].colour"),
                    "the sRGB is derived from the colour index b_minus_v, saying how, or a choice \
                     with its reason, not both",
                );
            }
        }
        self.evidence().check()
    }

    /// The free text beside what its citations show.
    fn evidence(&self) -> Evidence<'_> {
        let mut evidence = Evidence::default();
        for paper in &self.papers {
            evidence.text(&paper.reference);
        }
        evidence.free("notes", &self.notes);
        evidence.free("points_basis", &self.points_basis);
        for (field, number, _) in self.fields() {
            match number {
                Number::Cited(c) => evidence.value(&field, c),
                Number::Choice(c) => {
                    evidence.numbers([c.value]);
                    evidence.free(format!("{field}.choice"), &c.choice);
                }
            }
        }
        for (i, p) in self.populations.iter().enumerate() {
            for (name, text) in [("derived", &p.colour.derived), ("choice", &p.colour.choice)] {
                if let Some(text) = text {
                    evidence.free(format!("populations[{i}].colour.{name}"), text);
                }
            }
        }
        evidence
    }

    /// The model the definition gives, each number in the unit the model reads it in.
    ///
    /// # Errors
    ///
    /// Fails on a number whose unit is not of its dimension, which [`ModelFile::read`] refuses.
    pub fn model(&self) -> Result<Model, String> {
        let populations = self
            .populations
            .iter()
            .map(|pop| {
                Ok(Population {
                    code: pop.code,
                    id: pop.id.clone(),
                    name: pop.name.clone(),
                    density: density(&pop.density)?,
                    light: light(&pop.light)?,
                    colour: colour(&pop.colour)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let a = &self.arms;
        Ok(Model {
            points: self.points,
            seed: self.seed,
            r0: parameter(&self.r0, KPC)?,
            sun_height: parameter(&self.sun_height, KPC)?,
            populations,
            arms: ArmRules {
                order: a
                    .order
                    .iter()
                    .map(|o| ArmOrder {
                        arm: o.arm.slug().to_owned(),
                        structures: o.structures.clone(),
                    })
                    .collect(),
                extrapolation_weight: parameter(&a.extrapolation_weight, ONE)?,
                same_stretch: parameter(&a.same_stretch, KPC)?,
                old_arm_contrast: parameter(&a.old_arm_contrast, ONE)?,
                old_arm_spread: parameter(&a.old_arm_spread, ONE)?,
            },
            papers: self
                .papers
                .iter()
                .map(|paper| ModelPaper {
                    id: paper.id.clone(),
                    reference: paper.reference.clone(),
                    doi: paper.doi.clone(),
                })
                .collect(),
        })
    }
}

/// A density's numbers, each with its field under `density` and the unit the model reads it in.
fn density_numbers(density: &DensityFile) -> Vec<Field<'_>> {
    let field = |name: &str, n, unit| (name.to_owned(), n, unit);
    match density {
        DensityFile::ExponentialDisc {
            scale_length,
            scale_height,
            max_radius,
            ..
        } => vec![
            field("exponential_disc.scale_length", scale_length, KPC),
            field("exponential_disc.scale_height", scale_height, KPC),
            field("exponential_disc.max_radius", max_radius, KPC),
        ],
        DensityFile::Bar {
            angle,
            bulge_scales,
            long_bar: b,
            long_bar_share,
        } => {
            let mut out = vec![field("bar.angle", angle, DEG)];
            for (k, s) in bulge_scales.iter().enumerate() {
                out.push((format!("bar.bulge_scales[{k}]"), s, KPC));
            }
            for (name, n, unit) in [
                ("angle", &b.angle, DEG),
                ("x0", &b.x0, KPC),
                ("y0", &b.y0, KPC),
                ("z0", &b.z0, KPC),
                ("c_perp", &b.c_perp, ONE),
                ("r_out", &b.r_out, KPC),
                ("sigma_out", &b.sigma_out, KPC),
                ("r_in", &b.r_in, KPC),
                ("sigma_in", &b.sigma_in, KPC),
            ] {
                out.push((format!("bar.long_bar.{name}"), n, unit));
            }
            out.push(field("bar.long_bar_share", long_bar_share, ONE));
            out
        }
        DensityFile::Halo {
            inner_slope,
            outer_slope,
            break_radius,
            flattening,
            min_radius,
            max_radius,
        } => vec![
            field("halo.inner_slope", inner_slope, ONE),
            field("halo.outer_slope", outer_slope, ONE),
            field("halo.break_radius", break_radius, KPC),
            field("halo.flattening", flattening, ONE),
            field("halo.min_radius", min_radius, KPC),
            field("halo.max_radius", max_radius, KPC),
        ],
        DensityFile::Arms {
            scale_height,
            width_factor,
            radial_scale_length,
            knots,
        } => {
            let mut out = vec![
                field("arms.scale_height", scale_height, KPC),
                field("arms.width_factor", width_factor, ONE),
                field("arms.radial_scale_length", radial_scale_length, KPC),
            ];
            if let Some(k) = knots {
                out.push(field("arms.knots.per_knot", &k.per_knot, ONE));
                out.push(field("arms.knots.knot_radius", &k.knot_radius, KPC));
            }
            out
        }
    }
}

/// A light's numbers, each with its field and the unit the model reads it in.
fn light_numbers(light: &LightFile) -> Vec<Field<'_>> {
    match light {
        LightFile::ByMass {
            mass,
            mass_to_light,
        } => vec![
            ("light.by_mass.mass".to_owned(), mass, Unit::SolarMass),
            (
                "light.by_mass.mass_to_light".to_owned(),
                mass_to_light,
                Unit::SolarMassPerSolarLuminosity,
            ),
        ],
        LightFile::Share(share) => vec![("light.share".to_owned(), share, ONE)],
    }
}

/// The model's density of a population's `density`.
fn density(density: &DensityFile) -> Result<Density, String> {
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
fn light(light: &LightFile) -> Result<Light, String> {
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
fn colour(colour: &ColourFile) -> Result<Colour, String> {
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
fn parameter(number: &Number, unit: Unit) -> Result<Parameter, String> {
    let factor = number.unit().factor_to(unit).map_err(|e| e.to_string())?;
    let (kind, source) = match number {
        Number::Cited(c) => (
            Some(c.kind),
            ParameterSource::Cited {
                paper: c.cite.paper.clone(),
                locator: locator_text(&c.cite.locator),
            },
        ),
        Number::Choice(c) => (
            None,
            ParameterSource::Choice {
                reason: c.choice.clone(),
            },
        ),
    };
    Ok(Parameter {
        value: number.value() * factor,
        unit: unit_name(unit),
        kind,
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
