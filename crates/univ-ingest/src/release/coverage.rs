//! Each source's primary reference and coverage as a `source_details` row: the sky its published
//! entries cover, as `HEALPix` cells (see [`super::sky`]), and the range of each radial measure
//! they hold, over the entries the release publishes from it.
//!
//! The radial measures are those an entry holds, each over its normalized values as the
//! `measurements` table gives them, nothing derived from them:
//!
//! - **redshift**: z in the frame each entry's source published it, as `measurements.value`
//!   holds it (cz / c where the source gives the velocity cz), not moved to the CMB frame;
//! - **distance**: the normalized distance, in the unit of the source's scale domain
//!   (`crate::tiles::unit_of`: pc for stars, Mpc otherwise), as published (a modulus as its
//!   luminosity distance, a length in h⁻¹ Mpc as the length for H0 = 100), not rescaled to the
//!   release cosmology nor made comoving;
//! - **parallax**: the trigonometric parallax in milliarcseconds, negative ones kept.
//!
//! An entry with several values of one measure counts once in its `entries`.

use std::collections::{BTreeMap, BTreeSet};

use univ_model::api::sources::{RadialRange, SkyFootprint};
use univ_model::entry::{CatalogueEntry, Position};
use univ_model::ids::SourceId;
use univ_model::release::rows::SourceDetailRow;
use univ_model::units::Unit;

use super::objects::{json as to_text, snake};
use super::sky::{COVERAGE_ORDER, cell};
use super::terms::variant_name;
use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::tiles::unit_of;

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

/// What a source's entries cover, gathered one entry at a time, its distances in one unit.
#[derive(Debug, Clone)]
pub struct Coverage {
    /// The unit of the distances: that of the source's scale domain.
    unit: Unit,
    cells: BTreeSet<u64>,
    redshift: Span,
    /// Redshifts by the frame they were published in, and those published as cz.
    redshift_frames: BTreeMap<&'static str, u64>,
    as_velocity: u64,
    distance: Span,
    /// The measures of the distances, and how many assume a cosmology, by the bits of its H0.
    distance_measures: BTreeSet<&'static str>,
    distance_assumed: BTreeMap<u64, u64>,
    parallax: Span,
}

impl Coverage {
    /// The coverage of no entry yet, its distances to be given in `unit` (pc or Mpc).
    #[must_use]
    pub fn new(unit: Unit) -> Self {
        Self {
            unit,
            cells: BTreeSet::new(),
            redshift: Span::default(),
            redshift_frames: BTreeMap::new(),
            as_velocity: 0,
            distance: Span::default(),
            distance_measures: BTreeSet::new(),
            distance_assumed: BTreeMap::new(),
            parallax: Span::default(),
        }
    }

    /// Adds `entry`, on the sky at its own position, else at `object_at`, the position of the
    /// object it joined: an entry attached by identifier, such as a spectral type by HIP number,
    /// covers the sky of the stars it describes.
    pub fn add(&mut self, entry: &CatalogueEntry, object_at: Option<&Position>) {
        if let Some(p) = entry.position.as_ref().or(object_at) {
            self.cells
                .insert(cell(COVERAGE_ORDER, p.icrs.ra, p.icrs.dec));
        }
        self.redshift
            .add(entry.redshifts.iter().map(|z| z.cmb.as_published().value));
        for z in &entry.redshifts {
            let frame = variant_name(&z.cmb.published).unwrap_or_default();
            *self.redshift_frames.entry(frame).or_default() += 1;
            self.as_velocity += u64::from(z.velocity.is_some());
        }
        let unit = self.unit;
        self.distance.add(
            entry
                .distances
                .iter()
                .filter_map(|d| d.normalized.to(unit).ok().map(|q| q.value)),
        );
        for d in &entry.distances {
            self.distance_measures
                .insert(variant_name(&d.measure).unwrap_or_default());
            if let Some(a) = &d.assumed_cosmology {
                *self
                    .distance_assumed
                    .entry(a.hubble_constant.to_bits())
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

    /// The `sky` column: the cells, or none when no entry has a sky position.
    #[must_use]
    pub fn sky(&self) -> Option<SkyFootprint> {
        (!self.cells.is_empty()).then(|| SkyFootprint {
            order: COVERAGE_ORDER,
            cells: self.cells.iter().copied().collect(),
        })
    }

    /// The `radial` column: one item per measure the entries hold, or none.
    #[must_use]
    pub fn radial(&self) -> Option<Vec<RadialRange>> {
        let mut items = Vec::new();
        let mut push = |measure: &str, span: &Span, unit: Unit, basis: &dyn Fn() -> String| {
            if span.entries > 0 {
                items.push(RadialRange {
                    measure: measure.to_owned(),
                    min: span.min,
                    max: span.max,
                    unit: snake(&unit),
                    basis: basis(),
                    entries: span.entries,
                });
            }
        };
        push("redshift", &self.redshift, Unit::Dimensionless, &|| {
            self.redshift_basis()
        });
        push("distance", &self.distance, self.unit, &|| {
            self.distance_basis()
        });
        push("parallax", &self.parallax, Unit::Milliarcsecond, &|| {
            "trigonometric parallax as published, in milliarcseconds, negative values kept"
                .to_owned()
        });
        (!items.is_empty()).then_some(items)
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

    fn distance_basis(&self) -> String {
        let measures: Vec<&str> = self.distance_measures.iter().copied().collect();
        let assumed: Vec<String> = self
            .distance_assumed
            .iter()
            .map(|(h, n)| format!("{n} assume H0 = {}", f64::from_bits(*h)))
            .collect();
        let assumed = if assumed.is_empty() {
            String::new()
        } else {
            format!(" ({})", assumed.join(", "))
        };
        format!(
            "distance in {} as normalized ({} distances as published), not rescaled to the \
             release cosmology nor made comoving{assumed}",
            snake(&self.unit),
            measures.join(", ")
        )
    }
}

/// The coverage of each source over the published entries of `objects`, its distances in the
/// unit of the source's scale domain among `definitions` (Mpc for a source not among them).
#[must_use]
pub fn coverages(
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
) -> BTreeMap<SourceId, Coverage> {
    let units: BTreeMap<&SourceId, Unit> = definitions
        .iter()
        .map(|d| (&d.id, unit_of(d.scale_domain)))
        .collect();
    let mut out: BTreeMap<SourceId, Coverage> = BTreeMap::new();
    for o in objects {
        let object_at = o
            .object
            .preferred_position
            .as_ref()
            .and_then(|p| o.object.entry(&p.entry))
            .and_then(|e| e.entry.position.as_ref());
        for e in &o.object.entries {
            let source = &e.entry.designation.source;
            out.entry(source.clone())
                .or_insert_with(|| {
                    Coverage::new(units.get(source).copied().unwrap_or(Unit::Megaparsec))
                })
                .add(&e.entry, object_at);
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
    (0u32..)
        .zip(definitions)
        .map(|(n, d)| {
            let coverage = coverages.get(&d.id);
            Ok(SourceDetailRow {
                release,
                source: n,
                bibcode: d.reference.bibcode.clone(),
                doi: d.reference.doi.clone(),
                reference_note: d.reference.none.clone(),
                sky: coverage
                    .and_then(Coverage::sky)
                    .as_ref()
                    .map(to_text)
                    .transpose()?,
                radial: coverage
                    .and_then(Coverage::radial)
                    .as_ref()
                    .map(to_text)
                    .transpose()?,
            })
        })
        .collect()
}
