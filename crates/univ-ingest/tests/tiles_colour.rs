//! The colour index a stars point carries (build-octree-tiles): which of its object's colour
//! indices is chosen, or that it has none. Every entry here is SIMULATED.

mod common;
mod simulated;

use simulated::tiles::object as release_object;
use simulated::{bare, value};
use univ_ingest::tiles::colour_of;
use univ_model::entry::{CatalogueEntry, ColourIndex};
use univ_model::object::Object;
use univ_model::tile::{ColourBands, PointColour};
use univ_model::units::Unit;
use univ_model::value::Uncertainty;

/// SIMULATED: a colour index of `value` magnitudes between `bands`, derived from two
/// magnitudes where `derived`.
fn colour(bands: [&str; 2], index: f64, derived: bool) -> ColourIndex {
    ColourIndex {
        bands: bands.map(str::to_owned),
        value: value(
            index,
            Unit::Magnitude,
            Unit::Magnitude,
            Uncertainty::NoneReported,
        ),
        derived_from: derived.then(|| ["BPmag".to_owned(), "RPmag".to_owned()]),
    }
}

/// SIMULATED: an object of one entry per source, each with the colour indices given.
fn object(entries: Vec<(&str, Vec<ColourIndex>)>) -> Object {
    let entries = entries
        .into_iter()
        .map(|(source, colours)| CatalogueEntry {
            colours: colours.into(),
            ages: thin_vec::ThinVec::new(),
            ..bare(source, "1")
        })
        .collect();
    *release_object(1, entries, None).object
}

fn chosen(o: &Object) -> Option<(f64, ColourBands)> {
    colour_of(o).map(|PointColour { index, bands }| (index, bands))
}

#[test]
fn a_point_takes_gaia_bp_rp_then_hipparcos_b_v_then_a_derived_bp_rp() {
    let gcns = ("gcns", vec![colour(["BP", "RP"], 0.9, true)]);
    let hip = (
        "hipparcos-2007",
        vec![
            colour(["B", "V"], 0.65, false),
            colour(["V", "I"], 0.7, false),
        ],
    );
    let gaia = ("gaia-dr3-bright", vec![colour(["BP", "RP"], 0.82, false)]);
    // Each before the next, whatever order the entries come in.
    let all = object(vec![gcns.clone(), hip.clone(), gaia]);
    assert_eq!(chosen(&all), Some((0.82, ColourBands::BpRp)));
    let no_gaia = object(vec![gcns.clone(), hip]);
    assert_eq!(chosen(&no_gaia), Some((0.65, ColourBands::BV)));
    let derived_only = object(vec![gcns]);
    assert_eq!(chosen(&derived_only), Some((0.9, ColourBands::BpRp)));
}

#[test]
fn a_point_whose_object_has_no_colour_index_the_tiles_code_carries_none() {
    assert_eq!(chosen(&object(vec![("twomrs", Vec::new())])), None);
    // V−I is a colour index, but not one version 2 codes.
    let v_i = object(vec![(
        "hipparcos-2007",
        vec![colour(["V", "I"], 0.7, false)],
    )]);
    assert_eq!(chosen(&v_i), None);
}
