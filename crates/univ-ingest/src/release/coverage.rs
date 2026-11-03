//! Each source's primary reference and coverage as a `source_details` row: the sky its published
//! entries cover, as `HEALPix` cells (see [`super::sky`]), and the range of each radial measure
//! they hold, over the entries the release publishes from it.
//!
//! The radial measures are those an entry holds, each over its normalized values as the
//! `measurements` table gives them, nothing derived from them:
//!
//! - **redshift**: z in the frame each entry's source published it, as `measurements.value`
//!   holds it (cz / c where the source gives the velocity cz), not moved to the CMB frame;
//! - **distance**: the normalized distance, in pc for a source of the `stars` scale domain and
//!   Mpc otherwise, as published (a modulus as its luminosity distance, a length in h⁻¹ Mpc as
//!   the length for H0 = 100), not rescaled to the release cosmology nor made comoving;
//! - **parallax**: the trigonometric parallax in milliarcseconds, negative ones kept.
//!
//! An entry with several values of one measure counts once in its `entries`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::release::rows::SourceDetailRow;
use univ_model::units::Unit;

use super::objects::{json as to_text, snake};
use super::sky::{COVERAGE_ORDER, cell};
use crate::crossmatch::ReleaseObject;
use crate::source::{ScaleDomain, SourceDefinition};

/// The range of one measure over a source's entries.
#[derive(Debug, Clone, Copy)]
struct Span {
    min: f64,
    max: f64,
    entries: u64,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            entries: 0,
        }
    }
}

impl Span {
    /// Adds one entry's values of the measure; an entry with none, or none finite, adds nothing.
    fn add(&mut self, values: impl IntoIterator<Item = f64>) {
        let mut any = false;
        for v in values.into_iter().filter(|v| v.is_finite()) {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
            any = true;
        }
        self.entries += u64::from(any);
    }
}

/// What a source's entries cover, gathered one entry at a time.
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    cells: BTreeSet<u64>,
    redshift: Span,
    /// Redshifts by the frame they were published in, and those published as cz.
    redshift_frames: BTreeMap<String, u64>,
    as_velocity: u64,
    distance: Span,
    /// The measures of the distances, and how many assume a cosmology, by its H0.
    distance_measures: BTreeSet<String>,
    distance_assumed: BTreeMap<String, u64>,
    parallax: Span,
}

impl Coverage {
    /// Adds `entry`, its distances in `unit` (pc or Mpc).
    pub fn add(&mut self, entry: &CatalogueEntry, unit: Unit) {
        if let Some(p) = &entry.position {
            self.cells
                .insert(cell(COVERAGE_ORDER, p.icrs.ra, p.icrs.dec));
        }
        self.redshift
            .add(entry.redshifts.iter().map(|z| z.cmb.as_published().value));
        for z in &entry.redshifts {
            *self
                .redshift_frames
                .entry(snake(&z.cmb.published))
                .or_default() += 1;
            self.as_velocity += u64::from(z.velocity.is_some());
        }
        self.distance.add(
            entry
                .distances
                .iter()
                .filter_map(|d| d.normalized.to(unit).ok().map(|q| q.value)),
        );
        for d in &entry.distances {
            self.distance_measures.insert(snake(&d.measure));
            if let Some(a) = &d.assumed_cosmology {
                *self
                    .distance_assumed
                    .entry(format!("H0 = {}", a.hubble_constant))
                    .or_default() += 1;
            }
        }
        self.parallax.add(
            entry
                .parallax
                .iter()
                .filter_map(|p| p.normalized.to(Unit::Milliarcsecond).ok().map(|q| q.value)),
        );
    }

    /// The `sky` column: the cells as JSON, or none when no entry has a sky position.
    #[must_use]
    pub fn sky(&self) -> Option<Json> {
        (!self.cells.is_empty()).then(|| json!({"order": COVERAGE_ORDER, "cells": self.cells}))
    }

    /// The `radial` column: one item per measure the entries hold, or none.
    #[must_use]
    pub fn radial(&self, unit: Unit) -> Option<Json> {
        let mut items = Vec::new();
        let mut push = |measure: &str, span: &Span, unit: Unit, basis: String| {
            if span.entries > 0 {
                items.push(json!({
                    "measure": measure,
                    "min": span.min,
                    "max": span.max,
                    "unit": snake(&unit),
                    "basis": basis,
                    "entries": span.entries,
                }));
            }
        };
        push(
            "redshift",
            &self.redshift,
            Unit::Dimensionless,
            self.redshift_basis(),
        );
        push("distance", &self.distance, unit, self.distance_basis(unit));
        push(
            "parallax",
            &self.parallax,
            Unit::Milliarcsecond,
            "trigonometric parallax as published, in milliarcseconds, negative values kept"
                .to_owned(),
        );
        (!items.is_empty()).then_some(Json::Array(items))
    }

    fn redshift_basis(&self) -> String {
        let name = |frame: &str| match frame {
            "cmb" => "the CMB frame".to_owned(),
            "local_group" => "the Local Group frame".to_owned(),
            other => other.to_owned(),
        };
        let frames = if self.redshift_frames.len() == 1 {
            self.redshift_frames
                .keys()
                .map(|f| name(f))
                .collect::<String>()
        } else {
            let each: Vec<String> = self
                .redshift_frames
                .iter()
                .map(|(frame, n)| format!("{} for {n}", name(frame)))
                .collect();
            each.join(", ")
        };
        let velocity = if self.as_velocity > 0 {
            format!(
                "; {} published as the velocity cz, read as z = cz / c",
                self.as_velocity
            )
        } else {
            String::new()
        };
        format!(
            "redshift z as normalized, in the frame its source published it in ({frames}), \
             none moved between frames{velocity}"
        )
    }

    fn distance_basis(&self, unit: Unit) -> String {
        let measures: Vec<&str> = self.distance_measures.iter().map(String::as_str).collect();
        let assumed: Vec<String> = self
            .distance_assumed
            .iter()
            .map(|(h, n)| format!("{n} assume {h}"))
            .collect();
        let assumed = if assumed.is_empty() {
            String::new()
        } else {
            format!(" ({})", assumed.join(", "))
        };
        format!(
            "distance in {} as normalized ({} distances as published), not rescaled to the \
             release cosmology nor made comoving{assumed}",
            snake(&unit),
            measures.join(", ")
        )
    }
}

/// The unit a source's distances are given in: pc for a source of stars, Mpc otherwise.
#[must_use]
pub fn distance_unit(definition: &SourceDefinition) -> Unit {
    if definition.scale_domain == ScaleDomain::Stars {
        Unit::Parsec
    } else {
        Unit::Megaparsec
    }
}

/// The coverage of each source over the published entries of `objects`.
#[must_use]
pub fn coverages(
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
) -> BTreeMap<SourceId, Coverage> {
    let units: BTreeMap<&SourceId, Unit> = definitions
        .iter()
        .map(|d| (&d.id, distance_unit(d)))
        .collect();
    let mut out: BTreeMap<SourceId, Coverage> = BTreeMap::new();
    for o in objects {
        for e in &o.object.entries {
            let source = &e.entry.designation.source;
            let unit = units.get(source).copied().unwrap_or(Unit::Megaparsec);
            out.entry(source.clone()).or_default().add(&e.entry, unit);
        }
    }
    out
}

/// The `source_details` rows of release `release`: each of `definitions`, numbered by its place
/// among them as `sources.n` is, with its reference and its coverage among `coverages`.
///
/// # Errors
///
/// Fails when a value cannot be written as JSON.
pub fn source_detail_rows(
    release: u32,
    definitions: &[SourceDefinition],
    coverages: &BTreeMap<SourceId, Coverage>,
) -> Result<Vec<SourceDetailRow>, String> {
    let empty = Coverage::default();
    (0u32..)
        .zip(definitions)
        .map(|(n, d)| {
            let coverage = coverages.get(&d.id).unwrap_or(&empty);
            Ok(SourceDetailRow {
                release,
                source: n,
                bibcode: d.reference.bibcode.clone(),
                doi: d.reference.doi.clone(),
                reference_note: d.reference.none.clone(),
                sky: coverage.sky().as_ref().map(to_text).transpose()?,
                radial: coverage
                    .radial(distance_unit(d))
                    .as_ref()
                    .map(to_text)
                    .transpose()?,
            })
        })
        .collect()
}
