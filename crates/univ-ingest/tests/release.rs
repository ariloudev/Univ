//! Building a release (build-a-release) from the recorded answers of five `VizieR` sources and
//! the repository's researched structures: its number, build time, cosmology, versions and
//! rules (AC-82, AC-83), its manifest (AC-84), its cell index (AC-85), and its number following
//! the last (AC-82).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use common::workspace;
use release_fixture::{
    BUILT_AT, Fixture, build_next, built, cross, d1_text, fixture, read_manifest, tiles,
};
use univ_ingest::crossmatch::{Basis, CrossMatched};
use univ_ingest::release::{
    Placing, StructureBounds, cell_rows, list, placements, release_dir, structure_rows,
};
use univ_ingest::state::{State, pending};
use univ_ingest::tiles::sha256;
use univ_model::cosmology::Cosmology;
use univ_model::derive::MODEL_VERSION;
use univ_model::position::Frame;
use univ_model::redshift::CmbCorrection;
use univ_model::release::cells::STRUCTURE_SPAN;
use univ_model::release::manifest::Target;
use univ_model::release::{CellSpace, keys};

#[test]
fn a_release_records_its_number_build_time_cosmology_versions_and_rules() {
    let f = fixture();
    let release = build_next(&f);
    assert_eq!(release.number, 1);
    assert_eq!(release.dir, release_dir(f.out.path(), 1));
    let m = read_manifest(&release.dir);
    assert_eq!(m, release.manifest);
    assert_eq!(m.release, 1);
    assert_eq!(m.built_at, BUILT_AT);
    assert_eq!(m.cosmology, Cosmology::planck_2018());
    assert_eq!(m.peculiar_velocity_km_s, 300.0);
    assert_eq!(m.versions.univ_model, MODEL_VERSION);
    assert_eq!(m.versions.univ_ingest, env!("CARGO_PKG_VERSION"));

    // The matching rules and the ranking rule, recorded verbatim with their digests.
    for name in ["matching", "ranking", "tiles"] {
        let repo = std::fs::read(workspace().join(format!("rules/{name}.yaml"))).unwrap();
        let rule = m.rules.iter().find(|r| r.name == name).unwrap();
        assert_eq!(rule.sha256, sha256(&repo), "{name}");
        assert_eq!(std::fs::read(release.dir.join(&rule.path)).unwrap(), repo);
    }
    let ranking = m.rules.iter().find(|r| r.name == "ranking").unwrap();
    assert_eq!(
        ranking.rule.as_deref(),
        Some(
            "measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method"
        )
    );
    // The releases row carries them too, for the API.
    let releases = d1_text(&release.dir, &m, "releases");
    assert!(
        releases.contains("'measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method'")
    );
    assert!(releases.contains(&format!("'{BUILT_AT}'")));
    assert!(
        releases.contains("tie_tolerance: 0.001"),
        "the ranking file's text"
    );
    assert!(
        releases.contains("structure_members:"),
        "the matching file's text"
    );

    // Every source is held, each with its review; all are still provisional.
    assert_eq!(release.provisional.len(), f.definitions.len());
    assert!(m.sources.iter().all(|s| s.review == "provisional"));
    let twomrs = m.sources.iter().find(|s| s.id == "twomrs").unwrap();
    assert_eq!(twomrs.entries, f.normalized[3].report.published);
}

#[test]
fn the_manifest_lists_every_file_of_the_release_with_its_sha256() {
    let f = fixture();
    let release = build_next(&f);
    let m = read_manifest(&release.dir);
    let mut listed: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
    listed.push(keys::MANIFEST.to_owned());
    listed.sort();
    assert_eq!(list(&release.dir, &release.dir).unwrap(), listed);
    for file in &m.files {
        let bytes = std::fs::read(release.dir.join(&file.path)).unwrap();
        assert_eq!(file.bytes, bytes.len() as u64, "{}", file.path);
        assert_eq!(file.sha256, sha256(&bytes), "{}", file.path);
        match file.target {
            Target::R2 => assert_eq!(file.key, Some(format!("releases/1/{}", file.path))),
            Target::D1 => {
                assert!(file.path.starts_with("d1/") && file.key.is_none());
            }
        }
    }
    for path in [
        "d1/catalogue/0000-schema.sql",
        "d1/shard-00/0000-schema.sql",
        "tiles/stars/manifest.json",
        "tiles/cosmic_web/r.bin",
        "rules/release.yaml",
    ] {
        assert!(m.files.iter().any(|f| f.path == path), "{path}");
    }
    assert!(m.d1.estimated_bytes > 0 && m.d1.estimated_bytes < m.d1.cap_bytes);
}

/// The boxes the fixture's structures enter the cell index with.
fn structure_bounds(f: &Fixture, crossed: &CrossMatched) -> Vec<StructureBounds> {
    let release = univ_model::derive::Release::new(&crossed.cosmology);
    let dipole = CmbCorrection::new(crossed.dipole);
    let placing = Placing {
        release: &release,
        correction: &dipole,
        dipole: "planck2018".to_owned(),
    };
    let none = std::collections::BTreeMap::new();
    let placed = placements(&f.structures, &crossed.structures, &placing, &none).unwrap();
    structure_rows(1, &placed).unwrap().1
}

#[test]
fn the_cell_index_holds_every_placed_object_in_the_cell_of_its_position() {
    let f = fixture();
    let crossed = cross(&f);
    let mut bounds = structure_bounds(&f, &crossed);
    // SIMULATED: the recorded answers place no member list whole, so a wall 1,500 Mpc long
    // stands in for a structure with a bounding box.
    let wall = univ_model::geometry::Aabb {
        min: [-750.0, 100.0, 0.0],
        max: [750.0, 200.0, 10.0],
    };
    bounds.push(("simulated-wall".to_owned(), Ok(wall)));
    let cells = cell_rows(
        1,
        &crossed.objects,
        &f.definitions,
        &crossed.cosmology,
        &bounds,
    )
    .unwrap();
    let placed: Vec<_> = crossed
        .objects
        .iter()
        .filter(|o| o.position.is_some())
        .collect();
    assert_ne!(placed.len(), 0);
    assert_eq!(cells.objects.len(), placed.len());
    for o in placed {
        let row = cells
            .objects
            .iter()
            .find(|r| r.object_id == o.id.get())
            .unwrap();
        let space = match row.domain {
            "stars" => CellSpace::Stars,
            "nearby_galaxies" => CellSpace::NearbyGalaxies,
            _ => CellSpace::CosmicWeb,
        };
        let mpc = o.position.as_ref().unwrap().cartesian(Frame::Icrs).unwrap();
        let at = mpc.map(|c| {
            if space == CellSpace::Stars {
                c * 1e6
            } else {
                c
            }
        });
        let cell = space.cell(at).unwrap();
        assert_eq!(
            (row.ix, row.iy, row.iz),
            (cell.ix, cell.iy, cell.iz),
            "{}",
            o.id
        );
        // A query of a small box around the object reads its cell.
        let query = univ_model::geometry::Aabb {
            min: at.map(|c| c - 0.5),
            max: at.map(|c| c + 0.5),
        };
        assert!(space.range(&query).unwrap().contains(cell));
    }
    // Each structure with a bounding box takes at most 64 cells, which cover its box.
    for (id, b) in bounds
        .iter()
        .filter_map(|(id, b)| Some((id, *b.as_ref().ok()?)))
    {
        let rows: Vec<_> = cells
            .structures
            .iter()
            .filter(|r| &r.structure == id)
            .collect();
        assert!(
            !rows.is_empty() && i64::try_from(rows.len()).unwrap() <= STRUCTURE_SPAN.pow(3),
            "{id}"
        );
        let level = rows[0].level;
        let corner = CellSpace::Structures.cell_at(level, b.min).unwrap();
        assert!(
            rows.iter()
                .any(|r| (r.ix, r.iy, r.iz) == (corner.ix, corner.iy, corner.iz))
        );
    }
    assert_eq!(
        cells.structures_indexed,
        bounds.iter().filter(|(_, b)| b.is_ok()).count() as u64
    );
    assert_eq!(
        cells
            .structures
            .iter()
            .filter(|r| r.structure == "simulated-wall")
            .count(),
        4,
        "4 cells of 640 Mpc along x, at level 6"
    );

    // The built release holds the index as its D1 rows.
    let tiles = tiles(&f, &crossed);
    let release = built(&f, &crossed, &tiles, &f.definitions, &f.structures).unwrap();
    let m = &release.manifest;
    assert_eq!(m.counts.object_cells, m.counts.placed);
    let rows = d1_text(&release.dir, m, "object_cells");
    assert_eq!(rows.matches("\n(1, ").count(), cells.objects.len());
}

#[test]
fn the_release_number_increments_and_the_pending_state_becomes_the_state() {
    let f = fixture();
    assert_eq!(build_next(&f).number, 1);
    assert!(!pending(f.state.path()).exists());
    let state = State::read(f.state.path()).unwrap();
    assert_eq!(state.ledger.releases().len(), 1);
    let second = build_next(&f);
    assert_eq!(second.number, 2);
    assert_eq!(
        second.manifest.ids.issued_first, None,
        "every object kept its id"
    );
    assert_eq!(
        State::read(f.state.path()).unwrap().ledger.releases()[1].name,
        "2"
    );
    assert!(release_dir(f.out.path(), 1).is_dir() && release_dir(f.out.path(), 2).is_dir());
}

#[test]
fn a_release_is_built_under_the_rules_and_structures_its_cross_match_read() {
    let f = fixture();
    // The repository's rules, read by the cross-match from a copy.
    let rules = tempfile::tempdir().unwrap();
    for name in ["matching", "ranking", "tiles", "release"] {
        let file = format!("{name}.yaml");
        std::fs::copy(
            workspace().join("rules").join(&file),
            rules.path().join(&file),
        )
        .unwrap();
    }
    let read = Basis::read(rules.path(), &[workspace().join("structures")]).unwrap();
    let mut crossed = cross(&f);
    crossed.basis = read.clone();
    let laid = tiles(&f, &crossed);
    // The files change after the cross-match read them.
    let ranking = std::fs::read_to_string(rules.path().join("ranking.yaml")).unwrap();
    let changed = ranking.replace("peculiar_velocity_km_s: 300", "peculiar_velocity_km_s: 250");
    assert_ne!(changed, ranking);
    std::fs::write(rules.path().join("ranking.yaml"), &changed).unwrap();
    std::fs::write(rules.path().join("matching.yaml"), "changed: true\n").unwrap();

    let release = built(&f, &crossed, &laid, &f.definitions, &read.structures).unwrap();
    let m = &release.manifest;
    assert_eq!(m.peculiar_velocity_km_s, 300.0);
    for name in ["matching", "ranking"] {
        let recorded = m.rules.iter().find(|r| r.name == name).unwrap();
        assert_eq!(recorded.sha256, sha256(read.file(name)), "{name}");
        let path = release.dir.join(&recorded.path);
        assert_eq!(std::fs::read(path).unwrap(), read.file(name), "{name}");
    }
    let row = d1_text(&release.dir, m, "releases");
    assert!(row.contains("peculiar_velocity_km_s: 300") && !row.contains("changed: true"));
    assert_eq!(m.counts.structures, read.structures.len() as u64);
}

#[test]
fn the_manifest_notes_the_distance_ladders_hubble_constant_beside_the_release_cosmology() {
    // The owner's decision: redshift distances keep Planck 2018, and the manifest says that
    // ladder-calibrated distances are on H0 = 74.6 +- 0.8 (Tully et al. 2023), citing both.
    let f = fixture();
    let m = read_manifest(&build_next(&f).dir);
    let note = m.cosmology_note.as_deref().unwrap();
    for needle in [
        "Planck Collaboration 2020, A&A 641, A6",
        "67.66",
        "Tully et al. 2023 (ApJ 944, 94",
        "74.6",
        "0.8",
        "about 11%",
    ] {
        assert!(note.contains(needle), "{needle}: {note}");
    }
    // It sits beside the cosmology in the manifest, as the release configuration gives it.
    let text = std::fs::read_to_string(release_dir(f.out.path(), 1).join("manifest.yaml")).unwrap();
    let at = |key: &str| text.find(&format!("\n{key}:")).unwrap();
    assert!(
        at("cosmology") < at("cosmology_note")
            && at("cosmology_note") < at("peculiar_velocity_km_s")
    );
}
