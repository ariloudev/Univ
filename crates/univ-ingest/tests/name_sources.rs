//! The name sources: the IAU Catalog of Star Names and `OpenNGC`, read from rows of their real
//! files (`tests/fixtures/names`, `tests/fixtures/github`), and cross-matched on SIMULATED
//! entries made up under the ids of real sources so that the repository's
//! `rules/matching.yaml` applies.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a failed unwrap or a panic is the test failing"
)]

mod common;
mod release_fixture;
mod simulated;

use std::path::{Path, PathBuf};

use common::{definition_of, entry};
use release_fixture::{build_next, fixture, loaded_in, record};
use simulated::{designation, entry as at_sky, matching_rules, offset, with_cz};
use univ_ingest::matching::{Matched, match_entries};
use univ_ingest::normalize::{Normalized, normalize};
use univ_ingest::source::LeftOutReason;
use univ_model::entry::{CatalogueEntry, Flags};
use univ_model::ids::Designation;
use univ_model::object::{Object, Resolution};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::release::shards::Database;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn correction() -> CmbCorrection {
    CmbCorrection::new(DipoleModel::Planck2018)
}

/// The IAU names of the fixture snapshot: Sirius, a star with no HIP number, and a host of an
/// exoplanet with neither a HIP number nor a position.
fn iau() -> Normalized {
    let file = fixtures().join("names/iau-wgsn.yaml");
    normalize(&definition_of("iau-star-names"), &[file], &correction()).unwrap()
}

/// The `OpenNGC` rows of the fixture, a piece of the prepared file.
fn openngc() -> Normalized {
    let file = fixtures().join("github/openngc.csv.gz");
    normalize(&definition_of("openngc"), &[file], &correction()).unwrap()
}

fn left(n: &Normalized, reason: LeftOutReason) -> u64 {
    n.report.left_out.get(&reason).copied().unwrap_or(0)
}

fn with_ids(mut e: CatalogueEntry, ids: &[(&str, &str)]) -> CatalogueEntry {
    e.cross_ids = ids.iter().copied().collect::<Flags>();
    e
}

fn object<'m>(m: &'m Matched, d: &Designation) -> &'m Object {
    &m.objects[m
        .position_of(d)
        .unwrap_or_else(|| panic!("no object holds {d:?}"))]
}

fn names(o: &Object) -> Vec<(&str, &str)> {
    let mut names: Vec<(&str, &str)> = o
        .names
        .iter()
        .map(|n| (n.source.as_str(), n.name.as_str()))
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn an_iau_name_keeps_its_hip_number_and_position_and_one_with_neither_is_left_out() {
    let n = iau();
    let sirius = entry(&n, "Sirius");
    assert_eq!(sirius.cross_ids.get("HIP"), Some("32349"));
    assert_eq!(sirius.name.as_deref(), Some("HR 2491"));
    assert!(sirius.position.is_some());
    // A name with no HIP number keeps its sky position.
    let butterfly = entry(&n, "Butterfly Star");
    assert!(butterfly.cross_ids.is_empty() && butterfly.position.is_some());
    // WASP-121's name: no HIP number and no position.
    assert_eq!(n.report.published, 2);
    assert_eq!(left(&n, LeftOutReason::MissingPosition), 1);
}

#[test]
fn openngc_rows_marked_duplicate_or_nonexistent_are_left_out_each_with_its_reason() {
    let n = openngc();
    assert_eq!(left(&n, LeftOutReason::DuplicateObject), 1, "M 102");
    assert_eq!(left(&n, LeftOutReason::NonexistentObject), 1, "IC 67");
    assert_eq!(n.report.published, 5);
    let m31 = entry(&n, "NGC 224");
    assert_eq!(m31.cross_ids.get("M"), Some("M 31"));
    assert_eq!(m31.cross_ids.get("PGC"), Some("002557"));
    assert_eq!(m31.cross_ids.get("2MASX"), Some("00424433+4116074"));
    assert_eq!(m31.name.as_deref(), Some("Andromeda Galaxy"));
    assert_eq!(m31.flags.get("type"), Some("G"));
    // A row with no cross-identifier carries none.
    assert!(entry(&n, "NGC 1976").cross_ids.get("PGC").is_none());
}

#[test]
fn an_iau_name_joins_the_star_its_hip_number_names_and_attaches_as_its_name() {
    // Simulated positions 2 degrees apart: the HIP number joins them, not the sky.
    let star = at_sky("hipparcos-2007", "32349", 101.28, -16.72);
    let named = with_ids(
        at_sky("iau-star-names", "Sirius", 103.0, -16.0),
        &[("HIP", "32349")],
    );
    let m = match_entries(&matching_rules(), vec![star, named]);
    assert_eq!(m.objects.len(), 1);
    let o = object(&m, &designation("hipparcos-2007", "32349"));
    let joined = o.entry(&designation("iau-star-names", "Sirius")).unwrap();
    assert!(matches!(
        &*joined.resolved_by,
        Resolution::CrossIdentification { scheme, .. } if scheme == "hip"
    ));
    assert_eq!(names(o), [("iau-star-names", "Sirius")]);
}

#[test]
fn a_galaxy_name_joins_the_object_its_pgc_number_or_2masx_designation_names() {
    // M31 (simulated positions): a Cosmicflows-4 galaxy by PGC, a 2MRS galaxy by 2MASX, each 2
    // arcmin from OpenNGC's centre, beyond any positional tolerance.
    let (ra, dec) = (10.6848, 41.2691);
    let (a, b) = (offset(ra, dec, 120.0, 0.0), offset(ra, dec, 0.0, 120.0));
    let ngc = with_ids(
        at_sky("openngc", "NGC 224", ra, dec),
        &[
            ("M", "M 31"),
            ("PGC", "002557"),
            ("2MASX", "00424433+4116074"),
        ],
    );
    let mut ngc = ngc;
    ngc.name = Some("Andromeda Galaxy".into());
    let m = match_entries(
        &matching_rules(),
        vec![
            ngc,
            at_sky("cosmicflows-4-galaxies", "2557", a.0, a.1),
            with_cz(at_sky("twompz", "00424433+4116074", b.0, b.1), -300.0, 50.0),
        ],
    );
    assert_eq!(m.objects.len(), 1, "{:?}", m.objects.len());
    let o = object(&m, &designation("openngc", "NGC 224"));
    assert!(
        o.entry(&designation("cosmicflows-4-galaxies", "2557"))
            .is_some()
    );
    assert!(
        o.entry(&designation("twompz", "00424433+4116074"))
            .is_some()
    );
    // Its Messier number is a designation the source gives beside its NGC name.
    assert!(
        o.designations()
            .any(|d| *d == designation("openngc", "M 31"))
    );
    assert_eq!(
        names(o),
        [
            ("openngc", "Andromeda Galaxy"),
            ("openngc", "M 31"),
            ("openngc", "NGC 224")
        ]
    );
}

#[test]
fn a_galaxy_name_with_no_shared_identifier_joins_the_object_its_position_matches() {
    let (ra, dec) = (226.6229, 55.7632);
    let near = offset(ra, dec, 4.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            at_sky("openngc", "NGC 5866", ra, dec),
            with_cz(
                at_sky("sdss-dr17-galaxies", "1", near.0, near.1),
                757.0,
                10.0,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 1);
}

#[test]
fn a_name_that_joins_nothing_is_an_object_of_its_own_with_each_of_its_names() {
    let (ra, dec) = (83.8187, -5.3897);
    let mut m42 = with_ids(at_sky("openngc", "NGC 1976", ra, dec), &[("M", "M 42")]);
    m42.name = Some("Great Orion Nebula,Orion Nebula".into());
    // A galaxy a degree away joins nothing.
    let far = offset(ra, dec, 3_600.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            m42,
            with_cz(
                at_sky("twomrs", "05380000-0523000", far.0, far.1),
                5000.0,
                30.0,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 2);
    let o = object(&m, &designation("openngc", "NGC 1976"));
    assert_eq!(o.entries.len(), 1);
    assert_eq!(
        names(o),
        [
            ("openngc", "Great Orion Nebula"),
            ("openngc", "M 42"),
            ("openngc", "NGC 1976"),
            ("openngc", "Orion Nebula")
        ]
    );
}

#[test]
fn a_release_names_m31_by_its_openngc_names_and_holds_m42_as_an_object_not_placed() {
    // The shared fixture, whose 2MRS answer holds M31 as 00424433+4116074, with the OpenNGC rows.
    let mut f = fixture();
    let n = openngc();
    record(f.cache.path(), "openngc", &n.report);
    f.definitions.push(definition_of("openngc"));
    f.normalized.push(n);
    let release = build_next(&f);
    let db = loaded_in(&release.dir, &release.manifest, Database::Catalogue);
    let names_of = |name: &str| -> Vec<String> {
        let mut q = db
            .prepare(
                "SELECT b.name FROM names a JOIN names b ON b.object_id = a.object_id \
                 WHERE a.name = ?1 ORDER BY b.name",
            )
            .unwrap();
        q.query_map([name], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(names_of("NGC 224"), ["Andromeda Galaxy", "M 31", "NGC 224"]);
    // M31 is the object 2MRS gives, joined by its 2MASX designation.
    let m31: i64 = db
        .query_row("SELECT object_id FROM names WHERE name = 'M 31'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let shard_of = |id: i64| -> i64 {
        db.query_row("SELECT shard FROM object_shards WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let shard = loaded_in(
        &release.dir,
        &release.manifest,
        Database::Shard(u32::try_from(shard_of(m31)).unwrap()),
    );
    // The shard holds the entries; the catalogue numbers their sources.
    let twomrs: i64 = db
        .query_row("SELECT n FROM sources WHERE id = 'twomrs'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let held: i64 = shard
        .query_row(
            "SELECT COUNT(*) FROM entries \
             WHERE object_id = ?1 AND source = ?2 AND identifier = '00424433+4116074'",
            [m31, twomrs],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(held, 1);
    // M42 joins nothing: an object of its own, not placed for having no distance.
    let m42: i64 = db
        .query_row("SELECT object_id FROM names WHERE name = 'M 42'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let shard = loaded_in(
        &release.dir,
        &release.manifest,
        Database::Shard(u32::try_from(shard_of(m42)).unwrap()),
    );
    let (placed, reason): (i64, i64) = shard
        .query_row(
            "SELECT placed, not_placed FROM objects WHERE id = ?1",
            [m42],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    // The reason is coded by the catalogue's terms.
    let reason: Option<String> = db
        .query_row("SELECT term FROM terms WHERE id = ?1", [reason], |r| {
            r.get(0)
        })
        .ok();
    assert_eq!((placed, reason.as_deref()), (0, Some("no_distance")));
    assert_eq!(
        names_of("M 42"),
        ["Great Orion Nebula", "M 42", "NGC 1976", "Orion Nebula"]
    );
}
