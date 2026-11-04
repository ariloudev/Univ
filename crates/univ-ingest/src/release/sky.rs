//! Regions of the sky as `HEALPix` cells: the NESTED scheme of Górski et al. 2005, "`HEALPix`: A
//! Framework for High-Resolution Discretization and Fast Analysis of Data Distributed on the
//! Sphere", `ApJ` 622, 759 (`2005ApJ...622..759G`), computed with the CDS `cdshealpix` crate
//! (version 0.10.1, Apache-2.0 OR MIT, F.-X. Pineau, Centre de Données astronomiques de
//! Strasbourg). Every cell is in the ICRS; a cell of order `k` is one of 12 × 4^k of equal area.
//!
//! - **A source's sky coverage** is the cells of order [`COVERAGE_ORDER`] that hold at least one
//!   of its entries' ICRS positions.
//! - **A structure's sky region**, a longitude and latitude box in its paper's frame, is the
//!   cells of order [`REGION_ORDER`] whose centres fall in the box, each centre moved from the
//!   ICRS into the box's frame (the inverse of the move that takes the box to the ICRS). An
//!   equatorial box of unstated equinox is read as J2000, as `super::place` places it.

use std::collections::BTreeSet;
use std::f64::consts::TAU;

use univ_model::position::{Frame, between, lon_lat, unit_vector};

use super::place::{SkyRanges, ranges};
use crate::structures::SkyBox;

/// The order of a source's coverage: 12,288 cells of about 3.4 square degrees (1.8° across),
/// enough to show a survey's footprint in at most 12,288 numbers.
pub const COVERAGE_ORDER: u8 = 5;

/// The order of a structure's sky region: 49,152 cells of about 0.84 square degrees (0.92°
/// across). It is the coarsest of orders 6 to 8, chosen so that the largest region of the
/// structure files, the `CfA2` Great Wall's 115.5° by 51° box (about 5,450 square degrees), is
/// 6,517 cells, a JSON array of about 40 kB within one D1 statement; order 7 would give it
/// 25,989. The smallest, the Hercules supercluster's 22.5 square degrees, still has 25.
pub const REGION_ORDER: u8 = 6;

/// The NESTED cell of order `order` holding the ICRS direction (`ra`, `dec`) in degrees.
#[must_use]
pub fn cell(order: u8, ra: f64, dec: f64) -> u64 {
    let lon = ra.to_radians().rem_euclid(TAU);
    let lat = dec.clamp(-90.0, 90.0).to_radians();
    cdshealpix::nested::hash(order, lon, lat)
}

/// The ICRS direction of a cell's centre, degrees.
#[must_use]
pub fn centre(order: u8, cell: u64) -> (f64, f64) {
    let (lon, lat) = cdshealpix::nested::center(order, cell);
    (lon.to_degrees(), lat.to_degrees())
}

/// The cells of order `order`, ascending, whose centres lie in `region`'s longitude and latitude
/// box, in its frame; or, for a box too small to hold a cell's centre, the cell that holds the
/// box's middle direction.
///
/// # Errors
///
/// Fails when a bound of the box is not an angle.
pub fn box_cells(region: &SkyBox, order: u8) -> Result<Vec<u64>, String> {
    let SkyRanges {
        lon0,
        span,
        lat0,
        lat1,
    } = ranges(region)?;
    let into_box = between(Frame::Icrs, region.frame);
    let inside = |(ra, dec): (f64, f64)| {
        let (lon, lat) = lon_lat(into_box.apply(unit_vector(ra, dec)));
        (lon - lon0).rem_euclid(360.0) <= span && (lat0..=lat1).contains(&lat)
    };
    let cells: Vec<u64> = (0..cdshealpix::nested::n_hash(order))
        .filter(|&c| inside(centre(order, c)))
        .collect();
    if !cells.is_empty() {
        return Ok(cells);
    }
    let middle = unit_vector(lon0 + span / 2.0, f64::midpoint(lat0, lat1));
    let (ra, dec) = lon_lat(between(region.frame, Frame::Icrs).apply(middle));
    Ok(vec![cell(order, ra, dec)])
}

/// The distinct cells of order `order` holding each ICRS direction of `directions`, ascending.
pub fn cells_of(order: u8, directions: impl IntoIterator<Item = (f64, f64)>) -> Vec<u64> {
    let set: BTreeSet<u64> = directions
        .into_iter()
        .map(|(ra, dec)| cell(order, ra, dec))
        .collect();
    set.into_iter().collect()
}
