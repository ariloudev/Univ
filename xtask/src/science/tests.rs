//! The check engine against a small fixture release.

mod fixture;

use std::path::Path;

use fixture::build;
use univ_model::release::{ReleaseManifest, TABLES, keys};

use super::{fingerprints, load, num, rules, science};

const CITE: &str = "cite: [{ paper: P, quote: Q, fetched: F }]";

/// The checks file used against the fixture: each kind once, holding or not as noted.
fn checks() -> String {
    let checks = [
        // Holds: 0.78 against 0.783 ± 0.025 at 2σ, placed by its Cepheid distance.
        "{ id: near, title: t, severity: fail, select: { kind: object_distance, object: { source: gal-a, identifier: A1 }, placed_by: { source: gal-a, method: cepheids } }, expect: { value: 0.783, sigma: 0.025 }, CITE }",
        // Does not hold: 0.78 Mpc against 20 ± 0.1, a concern.
        "{ id: far, title: t, severity: concern, select: { kind: object_distance, object: { name: Galaxy A } }, expect: { value: 20, absolute: 0.1 }, CITE }",
        "{ id: dir, title: t, severity: fail, select: { kind: object_direction, object: { source: gal-b, identifier: B1 } }, expect: { frame: icrs, lon: 10.005, lat: 41.005, within_arcmin: 0.01 }, CITE }",
        "{ id: pairs, title: t, severity: fail, select: { kind: close_pairs, scale_domains: [nearby_galaxies], kinds: [body], radius_arcmin: 1.0 }, expect: { equals: 0 }, CITE }",
        "{ id: pairs-excepted, title: t, severity: fail, select: { kind: close_pairs, scale_domains: [nearby_galaxies], kinds: [body], radius_arcmin: 1.0, exceptions: [{ source: gal-b, identifier: B1, reason: r }] }, expect: { equals: 0 }, CITE }",
        "{ id: inside-1, title: t, severity: fail, select: { kind: query, sql: \"SELECT id FROM objects WHERE release = :release AND placed = 1 AND distance_mpc < 1 AND scale_domain = term('nearby_galaxies')\", exceptions: [{ source: gal-a, identifier: A1, reason: r }] }, expect: { equals: 1 }, CITE }",
        "{ id: share, title: t, severity: fail, select: { kind: query, scalar: true, sql: \"SELECT avg(source = source('gal-a')) FROM entries WHERE release = :release\" }, expect: { at_least: 0.6, at_most: 0.7 }, CITE }",
        "{ id: grid, title: t, severity: fail, select: { kind: grid_labels, grid: g, field: 0, frame: supergalactic, unit: megaparsec_over_h, points: [{ name: w, at: [-1, -1, -1], label: 1 }, { name: e, at: [1, -1, 1], label: 2 }] }, expect: { equals: 0 }, CITE }",
        "{ id: tile, title: t, severity: fail, select: { kind: largest_tile }, expect: { at_most: 100 }, CITE }",
        "{ id: budget, title: t, severity: fail, select: { kind: d1_estimate }, expect: { at_most: 4500000000 }, CITE }",
        "{ id: fingerprints, title: t, severity: fail, select: { kind: fingerprints }, expect: { equals: 0 }, CITE }",
        "{ id: missing, title: t, severity: fail, select: { kind: object_distance, object: { source: gal-a, identifier: nope } }, expect: { value: 1, absolute: 1 }, CITE }",
        "{ id: report, title: t, severity: report, select: { kind: report, sql: \"SELECT placed, count(*) AS n FROM objects WHERE release = :release GROUP BY 1\" }, CITE }",
    ];
    let body: Vec<String> = checks
        .iter()
        .map(|c| format!("  - {}", c.replace("CITE", CITE)))
        .collect();
    format!("checks:\n{}\n", body.join("\n"))
}

#[test]
fn each_kind_of_check_is_judged_against_the_fixture_release() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());
    let rules = dir.path().join("checks.yaml");
    std::fs::write(&rules, checks()).unwrap();
    let args = [
        dir.path().to_string_lossy().into_owned(),
        "--rules".to_owned(),
        rules.to_string_lossy().into_owned(),
    ];
    let mut out = Vec::new();
    let passed = science(dir.path(), &args, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let status = |id: &str| {
        out.lines()
            .find(|l| l.split_whitespace().nth(1) == Some(id))
            .and_then(|l| l.split_whitespace().next())
            .unwrap_or_else(|| panic!("no line for {id} in\n{out}"))
            .to_owned()
    };
    let want = [
        ("near", "PASS"),
        ("far", "CONCERN"),
        ("dir", "PASS"),
        ("pairs", "FAIL"),
        ("pairs-excepted", "PASS"),
        ("inside-1", "PASS"),
        ("share", "PASS"),
        ("grid", "PASS"),
        ("tile", "PASS"),
        ("budget", "PASS"),
        ("fingerprints", "PASS"),
        ("missing", "FAIL"),
        ("report", "INFO"),
    ];
    for (id, s) in want {
        assert_eq!(status(id), s, "{id} in\n{out}");
    }
    assert!(!passed, "a FAIL fails the run");
    assert!(
        out.contains("1 [gal-a:A1] at 0.7800 ± 0.0200 Mpc and 2 [gal-b:B1]"),
        "{out}"
    );
    assert!(out.contains("placed=0 n=1"), "{out}");
    assert!(
        out.lines()
            .next()
            .unwrap()
            .starts_with("load: 2 D1 files of release 1")
    );
    assert!(
        out.lines()
            .last()
            .unwrap()
            .contains("13 checks: 9 PASS, 2 FAIL, 1 CONCERN, 1 INFO")
    );
}

#[test]
fn a_row_altered_after_the_build_is_found_by_the_fingerprints() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());
    let text = std::fs::read_to_string(dir.path().join(keys::MANIFEST)).unwrap();
    let manifest: ReleaseManifest = serde_norway::from_str(&text).unwrap();
    let (db, loaded) = load::open(None).unwrap();
    assert!(!loaded);
    assert_eq!(load::load(&db, dir.path(), &manifest).unwrap(), 2);
    let (tables, found) = fingerprints::differences(&db, &manifest).unwrap();
    assert_eq!((tables, found.len()), (TABLES.len(), 0), "{found:?}");
    // A distance moved, and a term renamed to another of the same length.
    db.execute_batch(
        "UPDATE objects SET distance_mpc = 0.781 WHERE id = 1; \
         UPDATE terms SET term = 'cepheidz' WHERE term = 'cepheids';",
    )
    .unwrap();
    let (_, found) = fingerprints::differences(&db, &manifest).unwrap();
    assert!(
        found.iter().any(|f| f.starts_with("objects.distance_mpc")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("terms: content SHA-256")),
        "{found:?}"
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_check_without_its_citation_or_tolerance_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let read = |yaml: &str| {
        let path = dir.path().join("c.yaml");
        std::fs::write(&path, yaml).unwrap();
        rules::read(&path)
    };
    let check = |select: &str, expect: &str, cite: &str| {
        format!(
            "checks:\n  - {{ id: a, title: t, severity: fail, select: {select}, expect: {expect}, cite: {cite} }}\n"
        )
    };
    let object = "{ kind: object_distance, object: { name: M31 } }";
    assert!(
        read(&check(
            object,
            "{ value: 1, sigma: 0.1 }",
            "[{ paper: P, quote: Q, fetched: F }]"
        ))
        .is_ok()
    );
    assert!(
        read(&check(
            object,
            "{ value: 1 }",
            "[{ paper: P, quote: Q, fetched: F }]"
        ))
        .is_err()
    );
    assert!(read(&check(object, "{ value: 1, sigma: 0.1 }", "[]")).is_err());
    assert!(
        read(&check(
            object,
            "{ value: 1, sigma: 0.1 }",
            "[{ paper: P, quote: '', fetched: F }]"
        ))
        .is_err()
    );
    assert!(
        read(&check(
            "{ kind: largest_tile }",
            "{ at_most: 1 }",
            "[{ paper: P, quote: Q, fetched: F }]"
        ))
        .is_ok()
    );
    assert!(
        read(&check(
            "{ kind: largest_tile, sql: x }",
            "{ at_most: 1 }",
            "[{ paper: P, quote: Q, fetched: F }]"
        ))
        .is_err()
    );
}

#[test]
fn the_checks_file_of_the_repository_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let file = rules::read(&root.join("rules/science-checks.yaml")).unwrap();
    assert!(file.checks.len() > 20);
}

#[test]
fn numbers_are_shown_whole_or_to_four_significant_digits() {
    assert_eq!(num(155.0), "155");
    assert_eq!(num(0.972_657), "0.9727");
    assert_eq!(num(16.767_8), "16.768");
    assert_eq!(num(111.374), "111.37");
    assert_eq!(num(1.25e-6), "1.25e-6");
}

#[test]
fn a_query_check_shows_as_many_rows_as_it_asks() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());
    let rules = dir.path().join("checks.yaml");
    let check = |id: &str, shown: &str| {
        format!(
            "  - {{ id: {id}, title: t, severity: concern, select: {{ kind: query, sql: \"SELECT id, distance_mpc FROM objects WHERE release = :release ORDER BY id\"{shown} }}, expect: {{ equals: 0 }}, {CITE} }}"
        )
    };
    let body = format!(
        "checks:\n{}\n{}\n",
        check("one", ", shown: 1"),
        check("all", "")
    );
    std::fs::write(&rules, body).unwrap();
    let args = [
        dir.path().to_string_lossy().into_owned(),
        "--rules".to_owned(),
        rules.to_string_lossy().into_owned(),
    ];
    let mut out = Vec::new();
    science(dir.path(), &args, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    // The rows listed under a check: the indented lines after its own.
    let listed = |id: &str| {
        out.lines()
            .skip_while(|l| l.split_whitespace().nth(1) != Some(id))
            .skip(1)
            .take_while(|l| l.starts_with("          "))
            .count()
    };
    assert!(listed("all") > 1, "{out}");
    assert_eq!(listed("one"), 1, "{out}");
}
