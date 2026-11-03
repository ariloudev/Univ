//! A void's region may hold none of the dense places its record names (the check of the earlier
//! Univ void outlines), on the SIMULATED grid, catalogues and void of `void/`: a simulated
//! cluster or supercluster inside the void refuses the outline, one outside it does not.
#![expect(
    clippy::unwrap_used,
    reason = "a failed unwrap in a test helper is the test failing"
)]

mod void;

use univ_ingest::outlines::{GridSky, Placed, dense_places, place_on};
use univ_ingest::structures::{DensePlaces, Structure};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{Object, ObjectEntry, Resolution};
use univ_model::position::Vec3;
use void::{VOID, catalogue, coordinates, everywhere, grid, id, read, reason, yaml};

/// The simulated void's record with a dense-place rule: clusters of `simulated-clusters` and
/// researched superclusters.
fn void_with_rule() -> Structure {
    let text = yaml(VOID, "simulated-full", "").replace(
        "    why: \"the numbers of this simulated test\"\n",
        "    why: \"the numbers of this simulated test\"\n    dense_places:\n      chosen_by: pipeline\n      sources: [simulated-clusters]\n      structure_kinds: [supercluster]\n      why: \"the rule of this simulated test\"\n",
    );
    read(&text).unwrap()
}

/// A simulated supercluster published at `at`, supergalactic h⁻¹ Mpc.
fn supercluster(at: Vec3) -> Structure {
    let text = format!(
        "name: Simulated Supercluster\nkind: supercluster\nstatus: observed\npapers:\n  - {{ id: simulated, reference: \"SIMULATED for this test: not a paper\", title: simulated, bibcode: simulated, read: \"not read: simulated\" }}\nposition: {}\nnotes: \"SIMULATED.\"\n",
        coordinates(at)
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("simulated-supercluster.yaml");
    std::fs::write(&file, text).unwrap();
    Structure::read(&file).unwrap()
}

/// An object of one simulated entry of `source` at `at`, with its redshift.
fn object(source: &str, at: Vec3) -> Object {
    let entry: CatalogueEntry = void::entry(source, 0, at, true);
    Object::new(vec![ObjectEntry {
        entry,
        resolved_by: Resolution::Anchor,
    }])
}

fn rule(s: &Structure) -> &DensePlaces {
    s.outline
        .as_ref()
        .unwrap()
        .test
        .dense_places
        .as_ref()
        .unwrap()
}

fn outline(objects: &[Object], structures: &[Structure]) -> univ_ingest::outlines::OutlineResult {
    let void = void_with_rule();
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    let catalogues = [(id("simulated-full"), &full[..])];
    let places = dense_places(rule(&void), objects, structures);
    let grid = grid();
    let sky = GridSky::new(&grid.geometry);
    place_on(&void, &grid, &sky, &catalogues, &places).unwrap()
}

#[test]
fn a_void_holding_a_dense_place_is_refused_and_shown_as_a_point_with_the_reason() {
    let near_centre = [VOID[0] + 4.0, VOID[1], VOID[2]];
    let result = outline(
        &[object("simulated-clusters", VOID)],
        &[supercluster(near_centre)],
    );
    let why = reason(&result).unwrap();
    assert!(
        why.contains("holds 2 of the dense places a void may not")
            && why.contains("simulated-clusters 0")
            && why.contains("Simulated Supercluster"),
        "{why}"
    );
    let check = result.dense.as_ref().unwrap();
    assert_eq!((check.given, check.on_grid, check.held.len()), (2, 2, 2));
    assert!(
        result.catalogues.is_empty(),
        "refused before the galaxy counts"
    );
}

#[test]
fn a_void_whose_dense_places_lie_outside_it_is_admitted() {
    let outside = [-VOID[0], VOID[1], VOID[2]];
    let result = outline(
        &[
            object("simulated-clusters", outside),
            // An object inside the void, but of a source the rule does not name.
            object("simulated-galaxies", VOID),
        ],
        &[supercluster(outside)],
    );
    let Placed::Outline { .. } = &result.placed else {
        panic!("refused: {:?}", reason(&result))
    };
    let check = result.dense.as_ref().unwrap();
    assert_eq!((check.given, check.on_grid), (2, 2));
    assert_eq!(check.held, Vec::<String>::new());
    let yaml = serde_norway::to_string(&result).unwrap();
    assert!(yaml.contains("chosen_by: pipeline"), "{yaml}");
}

#[test]
fn a_dense_place_off_the_grid_is_recorded_with_why_and_counted() {
    // A simulated supercluster published far beyond the simulated grid: it cannot be checked
    // against the region, and the record says so rather than dropping it.
    let far = [VOID[0] * 1_000.0, VOID[1], VOID[2]];
    let result = outline(&[object("simulated-clusters", VOID)], &[supercluster(far)]);
    let check = result.dense.as_ref().unwrap();
    assert_eq!((check.given, check.on_grid), (2, 1));
    assert_eq!(check.off_grid.len(), check.given - check.on_grid);
    let off = &check.off_grid[0];
    assert_eq!(off.name, "Simulated Supercluster");
    assert!(off.reason.contains("outside the grid"), "{}", off.reason);
    let yaml = serde_norway::to_string(&result).unwrap();
    assert!(yaml.contains("off_grid"), "{yaml}");
}

#[test]
fn only_a_void_states_dense_places_and_it_says_why() {
    let above = yaml(VOID, "simulated-full", "")
        .replace("side: below", "side: above")
        .replace(
            "underdense_in_every_covering_catalogue",
            "overdense_in_every_covering_catalogue",
        )
        .replace(
            "    why: \"the numbers of this simulated test\"\n",
            "    why: \"the numbers of this simulated test\"\n    dense_places: { chosen_by: pipeline, sources: [simulated-clusters], why: \"w\" }\n",
        );
    let e = read(&above).unwrap_err();
    assert!(
        e.problem.contains("dense places are a test of a void"),
        "{e}"
    );
    let unsaid = yaml(VOID, "simulated-full", "").replace(
        "    why: \"the numbers of this simulated test\"\n",
        "    why: \"the numbers of this simulated test\"\n    dense_places: { chosen_by: pipeline, sources: [simulated-clusters], why: \" \" }\n",
    );
    let e = read(&unsaid).unwrap_err();
    assert!(e.problem.contains("say why"), "{e}");
    let chosen_by_a_paper = yaml(VOID, "simulated-full", "").replace(
        "    why: \"the numbers of this simulated test\"\n",
        "    why: \"the numbers of this simulated test\"\n    dense_places: { chosen_by: paper, sources: [simulated-clusters], why: \"w\" }\n",
    );
    assert!(read(&chosen_by_a_paper).is_err());
}
