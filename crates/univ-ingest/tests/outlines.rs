//! Structures outlined from a volume grid (RQ-23, AC-58, AC-59), on a SIMULATED grid and
//! SIMULATED catalogues: nothing here is real data. The grid is 16 × 14 × 12 cells of
//! 10 h⁻¹ Mpc in supergalactic coordinates, stored Z slowest, then X, then Y, so that a reader
//! ignoring the axis order would find the wrong cells. Its density contrast is 0 except for a
//! simulated void of 19 cells at −1.5 around (35, 5, 5) h⁻¹ Mpc. Simulated galaxies sit at cell
//! centres with CMB-frame redshifts |r| × 100 / c, and simulated sky-only entries (a position,
//! no redshift) mark where each simulated survey looked.

mod void;

use univ_ingest::outlines::{Coverage, Placed, place};
use univ_ingest::structures::{Threshold, read_all};
use univ_model::geometry::Geometry;
use univ_model::position::{Frame, dot};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;
use void::{
    VOID, catalogue, coordinates, everywhere, geometry, grid, id, in_void, read, reason, structure,
    yaml,
};

#[test]
fn an_outline_emptier_than_every_covering_catalogue_expects_is_admitted_as_a_reconstructed_grid_region()
 {
    // AC-58: the outline is the void's 19 cells, and each covering catalogue is compared with
    // the cells at the same distance outside it.
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    // A survey of low supergalactic latitudes, which misses the void's five cells nearest the
    // supergalactic pole.
    let south = catalogue("simulated-south", 3, 1, 0.0, |p| {
        p[2] < 0.2 * dot(p, p).sqrt()
    });
    let s = structure(VOID, "simulated-full, simulated-south", "");
    let catalogues = [
        (id("simulated-full"), &full[..]),
        (id("simulated-south"), &south[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    let Placed::Outline {
        frame,
        unit,
        kind,
        geometry: Geometry::GridRegion { cells, shape, .. },
    } = &result.placed
    else {
        panic!("not admitted: {result:?}")
    };
    assert_eq!(*frame, Frame::Supergalactic);
    assert_eq!(*unit, Unit::MegaparsecOverH);
    assert_eq!(*kind, KindOfValue::Reconstructed);
    assert_eq!(*shape, [16, 14, 12]);
    assert_eq!(cells.len(), 19);
    assert!(cells.contains(&[11, 7, 6]));
    let g = geometry();
    for c in cells {
        assert!(in_void(g.centre(c.map(|i| i as usize))));
    }
    let seed = result.seed.as_ref().unwrap();
    assert_eq!((seed.cell, seed.value), ([11, 7, 6], -1.5));
    let [full, south] = &result.catalogues[..] else {
        panic!("two checks")
    };
    assert_eq!(full.coverage, Coverage::Covers);
    assert_eq!(full.held, 0);
    assert!((full.expected - 3.0 * 19.0).abs() < 1e-9, "{full:?}");
    assert_eq!(full.passes, Some(true));
    assert_eq!(south.coverage, Coverage::Covers);
    assert_eq!((south.judged_cells, south.held), (14, 14), "{south:?}");
    assert!(south.ratio.unwrap() < 0.5, "{south:?}");
    // The outline, the grid, threshold, test, numbers and verdict are recorded with it.
    let yaml = serde_norway::to_string(&result).unwrap();
    for field in [
        "grid: simulated-density",
        "chosen: -1",
        "ratio:",
        "kind: grid_region",
    ] {
        assert!(yaml.contains(field), "{field} in {yaml}");
    }
}

#[test]
fn an_outline_a_covering_catalogue_finds_overdense_is_published_as_a_point_with_the_reason() {
    // AC-59: one catalogue finds the void empty, another finds it fuller than its surroundings.
    let empty = catalogue("simulated-empty", 3, 0, 0.0, everywhere);
    let full = catalogue("simulated-crowded", 2, 4, 0.0, everywhere);
    let s = structure(VOID, "simulated-empty, simulated-crowded", "");
    let catalogues = [
        (id("simulated-empty"), &empty[..]),
        (id("simulated-crowded"), &full[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    let why = reason(&result).unwrap();
    assert!(
        why.contains("simulated-crowded holds 76 galaxies against 38.0 expected, a ratio of 2.00"),
        "{why}"
    );
    assert!(!why.contains("simulated-empty"), "{why}");
    let Placed::Point { position, .. } = &result.placed else {
        unreachable!()
    };
    assert_eq!(Some(position.as_ref()), s.position.as_ref());
    assert_eq!(result.cells, 19);
}

#[test]
fn an_outline_no_catalogue_covers_is_published_as_a_point_with_the_reason() {
    // A survey of the other hemisphere, a survey that starts beyond the void, and a catalogue
    // that was not given: none covers the outline.
    let north = catalogue("simulated-north", 3, 0, 0.0, |p| p[0] < 0.0);
    let deep = catalogue("simulated-deep", 3, 0, 55.0, everywhere);
    let s = structure(
        VOID,
        "simulated-north, simulated-deep, simulated-missing",
        "",
    );
    let catalogues = [
        (id("simulated-north"), &north[..]),
        (id("simulated-deep"), &deep[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    assert_eq!(
        reason(&result).unwrap(),
        "no catalogue of the test covers the outline"
    );
    let coverage: Vec<&Coverage> = result.catalogues.iter().map(|c| &c.coverage).collect();
    assert!(
        matches!(coverage[0], Coverage::OutsideFootprint { covered_share } if *covered_share == 0.0)
    );
    assert!(matches!(coverage[1], Coverage::OutsideFootprint { .. }));
    assert!(result.catalogues[1].near > 50.0);
    assert_eq!(coverage[2], &Coverage::NotGiven);
}

#[test]
fn a_published_position_on_the_wrong_side_of_the_threshold_is_published_as_a_point() {
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    let catalogues = [(id("simulated-full"), &full[..])];
    let beside = [-35.0, 5.0, 5.0];
    let result = place(
        &structure(beside, "simulated-full", ""),
        &grid(),
        &catalogues,
    )
    .unwrap();
    let why = reason(&result).unwrap();
    assert!(
        why.starts_with("the published position lies in cell [4, 7, 6]"),
        "{why}"
    );
    assert!(
        why.contains("density is 0.000, not below the threshold -1"),
        "{why}"
    );
    assert_eq!(result.catalogues.len(), 0);
    // Off the grid, it is a point too.
    let far = structure([500.0, 0.0, 0.0], "simulated-full", "");
    let result = place(&far, &grid(), &catalogues).unwrap();
    assert!(reason(&result).unwrap().contains("outside the grid"));
    // A named minimum the record states seeds the region instead.
    let seed = format!(
        "  seed:\n    name: simulated minimum\n    position: {}\n",
        coordinates(VOID)
    );
    let named = structure(beside, "simulated-full", &seed);
    let result = place(&named, &grid(), &catalogues).unwrap();
    assert!(
        matches!(result.placed, Placed::Outline { .. }),
        "{result:?}"
    );
    assert_eq!(result.seed.unwrap().name, "simulated minimum");
}

#[test]
fn a_threshold_chosen_by_this_pipeline_says_so_and_why_and_is_never_read_as_a_citation() {
    let s = structure(VOID, "simulated-full", "");
    let outline = s.outline.as_ref().unwrap();
    let Threshold::Chosen(choice) = &outline.threshold else {
        panic!("not chosen")
    };
    assert_eq!(choice.chosen, -1.0);
    assert!(
        !s.cited_values()
            .iter()
            .any(|(field, _)| field == "outline.threshold")
    );
    // A chosen threshold without its reason, or a bare number, is refused.
    let chosen = "threshold: { chosen: -1, why: \"the level of this simulated test\" }";
    let text = yaml(VOID, "simulated-full", "");
    for (broken, says) in [
        ("threshold: { chosen: -1 }", "missing field `why`"),
        ("threshold: { chosen: -1, why: \" \" }", "says why"),
        ("threshold: -1", "cites nothing"),
    ] {
        let e = read(&text.replace(chosen, broken)).unwrap_err();
        assert_eq!(e.field, "outline.threshold", "{e}");
        assert!(e.problem.contains(says), "{e}");
    }
    // The Local Void's threshold is this pipeline's choice, said to be one.
    let structures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../structures");
    let local = read_all(&structures)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Local Void")
        .unwrap();
    let outline = local.outline.unwrap();
    assert_eq!(outline.grid.as_str(), "cf4pp-density");
    let Threshold::Chosen(choice) = &outline.threshold else {
        panic!("the Local Void's threshold is not marked as chosen")
    };
    assert_eq!(choice.chosen, -1.0);
    assert!(
        choice.why.contains("not a published level"),
        "{}",
        choice.why
    );
}
