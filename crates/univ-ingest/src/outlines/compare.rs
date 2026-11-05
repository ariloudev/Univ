//! The comparison of an outline with one galaxy catalogue: whether the catalogue covers it, and
//! the galaxies it holds against those expected from the cells at the same distance outside it.
//!
//! Galaxies are placed in velocity space: each at its normalized CMB-frame spectroscopic
//! redshift, cz / 100 h⁻¹ Mpc along its ICRS direction rotated into the grid's frame. No
//! cosmology is involved.

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use univ_model::entry::{CatalogueEntry, RedshiftKind};
use univ_model::grid::{Axis, GridGeometry};
use univ_model::ids::SourceId;
use univ_model::position::{Frame, between, dot, from_icrs, lon_lat};

use super::region::{Region, velocity_distance};
use crate::structures::{OutlineTest, Rule};

/// How coverage is decided: choices of this pipeline, recorded with every outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CoverageRule {
    pub footprint: &'static str,
    pub depth: &'static str,
    /// The least share of the outline's cells a catalogue must cover.
    pub covered_share: f64,
    /// The fewest covered cells outside the outline a distance shell needs to give an expectation.
    pub min_outside_cells: usize,
}

/// The coverage rule every outline is tested with.
pub const COVERAGE: CoverageRule = CoverageRule {
    footprint: "the sky patches, 2 degrees of ICRS right ascension by 1/60 of the range of \
                sin(declination) so that all have one area (about 3.8 square degrees), that hold \
                at least one of the catalogue's galaxies",
    depth: "between the 1st and 99th percentiles of the catalogue's galaxy distances cz/100",
    covered_share: 0.5,
    min_outside_cells: 20,
};

const RA_BINS: usize = 180;
const DEC_BINS: usize = 60;

/// What every comparison on one grid needs of each cell, computed once per grid rather than once
/// per catalogue: the sky patch of its centre's ICRS direction, its centre's distance, and the
/// distance shell, one cell wide, it falls in. Indexed as the grid's samples.
pub struct GridSky<'a> {
    geometry: &'a GridGeometry,
    patch: Vec<usize>,
    distance: Vec<f64>,
    shell: Vec<usize>,
}

impl<'a> GridSky<'a> {
    #[must_use]
    pub fn new(geometry: &'a GridGeometry) -> Self {
        let to_icrs = between(geometry.frame, Frame::Icrs);
        let n = geometry.cells();
        let (mut patch, mut distance, mut shell) = (vec![0; n], vec![0.0; n], vec![0; n]);
        let along = Axis::XYZ.map(|a| geometry.cells_along(a));
        for x in 0..along[0] {
            for y in 0..along[1] {
                for z in 0..along[2] {
                    let cell = [x, y, z];
                    let Some(i) = geometry.sample_index(cell) else {
                        continue;
                    };
                    let centre = geometry.centre(cell);
                    let r = dot(centre, centre).sqrt();
                    let (ra, dec) = lon_lat(to_icrs.apply(centre));
                    patch[i] = sky_patch(ra, dec);
                    distance[i] = r;
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a distance over a positive cell, floored"
                    )]
                    let s = (r / geometry.cell).floor() as usize;
                    shell[i] = s;
                }
            }
        }
        Self {
            geometry,
            patch,
            distance,
            shell,
        }
    }

    #[must_use]
    pub fn geometry(&self) -> &'a GridGeometry {
        self.geometry
    }
}

/// Whether a catalogue covers the outline, and if not, why.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "is")]
pub enum Coverage {
    Covers,
    /// The catalogue's entries were not given to the test.
    NotGiven,
    /// The catalogue has no galaxy with a position and a spectroscopic redshift.
    NoGalaxies,
    /// Its footprint and depth hold too small a share of the outline's cells.
    OutsideFootprint {
        covered_share: f64,
    },
    /// It expects too few galaxies inside the outline to judge it.
    TooSparse {
        expected: f64,
    },
}

/// One catalogue's comparison with the outline.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CatalogueCheck {
    pub source: SourceId,
    pub coverage: Coverage,
    /// Galaxies placed in velocity space: a position and a spectroscopic redshift.
    pub galaxies: usize,
    /// The catalogue's depth, h⁻¹ Mpc in velocity space.
    pub near: f64,
    pub far: f64,
    /// Outline cells covered and in a shell with enough covered cells outside.
    pub judged_cells: usize,
    pub held: u64,
    pub expected: f64,
    /// Galaxies held over galaxies expected, when the catalogue covers the outline.
    pub ratio: Option<f64>,
    /// Whether the catalogue meets the test's rule, when it covers the outline.
    pub passes: Option<bool>,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts of cells and galaxies are far below 2^52"
)]
fn real(n: usize) -> f64 {
    n as f64
}

/// The equal-area sky patch of an ICRS direction, in degrees.
fn sky_patch(ra: f64, dec: f64) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both are floored into [0, bins)"
    )]
    let bin = |x: f64, bins: usize| ((x * real(bins)).floor().max(0.0) as usize).min(bins - 1);
    let sin = f64::midpoint(dec.to_radians().sin(), 1.0);
    bin(ra / 360.0, RA_BINS) * DEC_BINS + bin(sin, DEC_BINS)
}

/// A catalogue's galaxies counted into the grid's cells, its footprint and its depth.
struct Placed {
    footprint: Vec<bool>,
    counts: HashMap<usize, u64>,
    galaxies: usize,
    near: f64,
    far: f64,
}

fn place<E: Borrow<CatalogueEntry>>(geometry: &GridGeometry, entries: &[E]) -> Placed {
    let rotation = from_icrs(geometry.frame);
    let mut footprint = vec![false; RA_BINS * DEC_BINS];
    let mut counts = HashMap::new();
    let mut distances = thin_vec::ThinVec::new();
    for entry in entries {
        let entry: &CatalogueEntry = entry.borrow();
        let Some(position) = &entry.position else {
            continue;
        };
        footprint[sky_patch(position.icrs.ra, position.icrs.dec)] = true;
        let Some(z) = entry
            .redshifts
            .iter()
            .find(|z| z.kind == RedshiftKind::Spectroscopic)
        else {
            continue;
        };
        let r = velocity_distance(z.cmb.value.value);
        distances.push(r);
        let at = rotation.apply(position.icrs.unit_vector()).map(|u| u * r);
        if let Some(i) = geometry.cell_at(at).and_then(|c| geometry.sample_index(c)) {
            *counts.entry(i).or_default() += 1;
        }
    }
    distances.sort_by(f64::total_cmp);
    let quantile = |q: f64| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole index in [0, len)"
        )]
        let i = (q * real(distances.len().saturating_sub(1))).round() as usize;
        distances.get(i).copied().unwrap_or(0.0)
    };
    Placed {
        footprint,
        counts,
        galaxies: distances.len(),
        near: quantile(0.01),
        far: quantile(0.99),
    }
}

/// Sums of galaxies and cells in one distance shell, inside and outside the outline.
#[derive(Default)]
struct Shell {
    held: u64,
    inside: usize,
    around: u64,
    outside: usize,
}

/// Compares the outline `region` of the grid of `sky` with one catalogue: `None` entries when
/// the catalogue was not given. The entries are held or borrowed.
#[must_use]
pub fn compare<E: Borrow<CatalogueEntry>>(
    sky: &GridSky<'_>,
    region: &Region,
    source: &SourceId,
    entries: Option<&[E]>,
    test: &OutlineTest,
) -> CatalogueCheck {
    let mut check = CatalogueCheck {
        source: source.clone(),
        coverage: Coverage::NotGiven,
        galaxies: 0,
        near: 0.0,
        far: 0.0,
        judged_cells: 0,
        held: 0,
        expected: 0.0,
        ratio: None,
        passes: None,
    };
    let Some(entries) = entries else {
        return check;
    };
    let placed = place(sky.geometry, entries);
    (check.galaxies, check.near, check.far) = (placed.galaxies, placed.near, placed.far);
    if placed.galaxies == 0 {
        check.coverage = Coverage::NoGalaxies;
        return check;
    }
    let (shells, covered_inside) = shells(sky, region, &placed);
    let share = real(covered_inside) / real(region.cells.len().max(1));
    if share < COVERAGE.covered_share {
        check.coverage = Coverage::OutsideFootprint {
            covered_share: share,
        };
        return check;
    }
    for shell in shells.values() {
        if shell.outside >= COVERAGE.min_outside_cells {
            check.judged_cells += shell.inside;
            check.held += shell.held;
            #[expect(clippy::cast_precision_loss, reason = "galaxy counts below 2^52")]
            let mean = shell.around as f64 / real(shell.outside);
            check.expected += mean * real(shell.inside);
        }
    }
    if check.expected < test.min_expected {
        check.coverage = Coverage::TooSparse {
            expected: check.expected,
        };
        return check;
    }
    check.coverage = Coverage::Covers;
    #[expect(clippy::cast_precision_loss, reason = "galaxy counts below 2^52")]
    let ratio = check.held as f64 / check.expected;
    check.ratio = Some(ratio);
    check.passes = Some(match test.rule {
        Rule::UnderdenseInEveryCoveringCatalogue => ratio < test.ratio,
        Rule::OverdenseInEveryCoveringCatalogue => ratio > test.ratio,
    });
    check
}

/// The galaxies and covered cells of each distance shell, one cell wide, that holds a covered
/// cell of the region, inside and outside the region; and the region's covered cells.
fn shells(sky: &GridSky<'_>, region: &Region, placed: &Placed) -> (BTreeMap<usize, Shell>, usize) {
    let geometry = sky.geometry;
    let covered = |i: usize| {
        (placed.near..=placed.far).contains(&sky.distance[i]) && placed.footprint[sky.patch[i]]
    };
    let count = |i: usize| placed.counts.get(&i).copied().unwrap_or(0);
    let mut inside = vec![false; geometry.cells()];
    let mut shells: BTreeMap<usize, Shell> = BTreeMap::new();
    let mut covered_inside = 0;
    for &cell in &region.cells {
        let Some(i) = geometry.sample_index(cell) else {
            continue;
        };
        inside[i] = true;
        if covered(i) {
            covered_inside += 1;
            let shell = shells.entry(sky.shell[i]).or_default();
            shell.held += count(i);
            shell.inside += 1;
        }
    }
    for (i, _) in inside.iter().enumerate().filter(|(_, inside)| !**inside) {
        // The cheap test first: most cells lie outside the region, few in its shells.
        if let Some(shell) = shells.get_mut(&sky.shell[i])
            && covered(i)
        {
            shell.around += count(i);
            shell.outside += 1;
        }
    }
    (shells, covered_inside)
}
