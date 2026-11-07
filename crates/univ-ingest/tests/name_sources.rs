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
use simulated::{bare, designation, entry as at_sky, matching_rules, offset, with_cz};
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

/// The IAU names of the fixture snapshot: Sirius; Izar and Pulcherrima, which share HIP 72105;
/// Zibbatu, whose HIP number a fix corrects; Mesarthim and the Butterfly Star, with no HIP
/// number; and a host of an exoplanet with neither a HIP number nor a position.
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
fn an_iau_name_keeps_its_hip_number_and_only_one_without_keeps_the_tables_position() {
    let n = iau();
    let sirius = entry(&n, "Sirius");
    assert_eq!(sirius.cross_ids.get("hip"), Some("32349"));
    assert_eq!(sirius.name.as_deref(), Some("Sirius"));
    // Its star's position comes from Hipparcos: the table's is not attributed.
    assert!(sirius.position.is_none());
    // A name with no HIP number keeps the table's sky position.
    let butterfly = entry(&n, "Butterfly Star");
    assert!(butterfly.cross_ids.is_empty() && butterfly.position.is_some());
    // WASP-121's name, Dilmun: no HIP number and no position.
    assert_eq!(left(&n, LeftOutReason::MissingPosition), 1);
}

#[test]
fn names_sharing_a_hip_number_are_one_entry_and_a_fix_corrects_a_wrong_number() {
    let n = iau();
    // Pulcherrima and Izar, the components of epsilon Bootis, in the table's order.
    let eps = entry(&n, "Pulcherrima");
    assert_eq!(eps.cross_ids.get("hip"), Some("72105"));
    assert_eq!(eps.name.as_deref(), Some("Pulcherrima, Izar"));
    assert!(
        n.entries
            .iter()
            .all(|e| &*e.designation.identifier != "Izar")
    );
    // Zibbatu's HIP 9353 of the list, corrected to 93537.
    assert_eq!(entry(&n, "Zibbatu").cross_ids.get("hip"), Some("93537"));
    assert_eq!(n.report.fixes.len(), 1);
    assert_eq!(n.report.fixes[0].entries_changed, 1);
    // Seven names: one left out, two of them one entry.
    assert_eq!(n.report.published, 5);
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
    // The name carries no position: the HIP number joins it.
    let star = at_sky("hipparcos-2007", "32349", 101.28, -16.72);
    let mut named = with_ids(bare("iau-star-names", "Sirius"), &[("hip", "32349")]);
    named.name = Some("Sirius".into());
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

#[test]
fn a_name_without_a_hip_number_joins_the_hipparcos_star_at_its_position() {
    // Mesarthim, gamma Arietis, 1 arcsec from HIP 8832 (simulated offset of that size).
    let (ra, dec) = (28.38256, 19.29385);
    let star = offset(ra, dec, 1.0, 0.0);
    let mut named = at_sky("iau-star-names", "Mesarthim", ra, dec);
    named.name = Some("Mesarthim".into());
    let m = match_entries(
        &matching_rules(),
        vec![at_sky("hipparcos-2007", "8832", star.0, star.1), named],
    );
    assert_eq!(m.objects.len(), 1);
}

#[test]
fn an_ngc_name_joins_the_nearby_galaxy_catalogues_by_common_name() {
    // NGC 55 as OpenNGC and UNGC write it, 20 arcmin apart (simulated), within the scheme's 30.
    let (ra, dec) = (3.7233, -39.1966);
    let far = offset(ra, dec, 1_200.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            at_sky("openngc", "NGC 55", ra, dec),
            at_sky("updated-nearby-galaxies", "NGC0055", far.0, far.1),
        ],
    );
    assert_eq!(m.objects.len(), 1);
    let o = object(&m, &designation("openngc", "NGC 55"));
    let joined = o
        .entry(&designation("updated-nearby-galaxies", "NGC0055"))
        .unwrap();
    assert!(matches!(
        &*joined.resolved_by,
        Resolution::CrossIdentification { scheme, .. } if scheme == "common-name"
    ));
}

#[test]
fn a_group_is_related_once_to_its_dominant_galaxy_however_many_sources_carry_its_number() {
    // A Cosmicflows-4 group whose dominant galaxy, PGC 2557, both Cosmicflows-4 galaxies and
    // OpenNGC carry (simulated positions).
    let (ra, dec) = (10.6848, 41.2691);
    let m = match_entries(
        &matching_rules(),
        vec![
            at_sky("cosmicflows-4", "2557", ra + 1.0, dec),
            at_sky("cosmicflows-4-galaxies", "2557", ra, dec),
            with_ids(at_sky("openngc", "NGC 224", ra, dec), &[("PGC", "002557")]),
        ],
    );
    let galaxy = object(&m, &designation("openngc", "NGC 224"));
    let of = galaxy
        .relations
        .iter()
        .filter(|r| r.kind == univ_model::object::RelationKind::DominantMemberOf)
        .count();
    assert_eq!(of, 1, "{:?}", galaxy.relations);
}

#[test]
fn every_cross_identifier_the_matching_rules_read_is_a_column_its_source_defines() {
    use univ_ingest::matching::rules::EntryField;
    use univ_ingest::source::Field;
    let rules = matching_rules();
    let read = rules
        .cross_identifications
        .iter()
        .flat_map(|s| s.members.iter().map(|m| (&m.source, &m.field)))
        .chain(rules.designations.iter().map(|d| (&d.source, &d.field)))
        .chain(rules.names.iter().map(|n| (&n.source, &n.field)));
    let mut checked = 0;
    for (source, field) in read {
        if let EntryField::CrossIdentifier(column) = field {
            let d = definition_of(source.as_str());
            assert!(
                d.columns
                    .iter()
                    .any(|c| &c.column == column && c.field == Field::CrossIdentifier),
                "{source} defines no cross_identifier column {column}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 5, "{checked}");
}
