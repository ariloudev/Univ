//! The researched arm tracers: the masers and molecular clouds that trace the spiral arms beyond
//! the reach of Reid et al. 2019, read from tracer files in which every number is cited, each an
//! object of the Galaxy tracers domain and a member of every structure of the arm its paper
//! assigns it to (record-a-researched-arm-tracer).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use common::{definition_of, structures_dir, workspace};
use release_fixture::{build_next, fixture, fixture_with, loaded, record, terms};
use univ_ingest::normalize::Normalized;
use univ_ingest::source::{ScaleDomain, SourceDefinition};
use univ_ingest::structures::{Arm, StructureGeometry, read_all_named};
use univ_ingest::tracers::{ARM_FLAG, ASSUMES_FLAG, TracerFile, normalize, read_all};
use univ_model::distance::DistanceKind;
use univ_model::object::ObjectKind;
use univ_model::value::KindOfValue;

/// The two sources of researched arm tracers, with the kind of object each lists.
const SOURCES: [(&str, ObjectKind); 2] = [
    ("researched-arm-masers", ObjectKind::StarFormingRegion),
    ("researched-arm-clouds", ObjectKind::MolecularCloud),
];

/// The directory of `definition`'s tracer files in the repository.
fn dir_of(definition: &SourceDefinition) -> PathBuf {
    let (_, path) = definition.access.repository().unwrap();
    workspace().join(path)
}

/// Each source of [`SOURCES`] by id, with its tracers normalized from the repository's files,
/// read once.
static READ: LazyLock<BTreeMap<&'static str, (SourceDefinition, Normalized)>> =
    LazyLock::new(|| {
        SOURCES
            .iter()
            .map(|(id, _)| {
                let d = definition_of(id);
                let n = normalize(&d, &dir_of(&d))
                    .map_err(|e| e.to_string())
                    .unwrap();
                (*id, (d, n))
            })
            .collect()
    });

/// Source `id`'s definition.
fn definition(id: &str) -> &'static SourceDefinition {
    &READ[id].0
}

/// Source `id`'s tracers, normalized.
fn normalized(id: &str) -> &'static Normalized {
    &READ[id].1
}

#[test]
fn every_tracer_file_reads_and_is_of_its_sources_kind_and_frame() {
    for (id, kind) in SOURCES {
        let d = definition(id);
        assert_eq!(d.object_kind, Some(kind), "{id}");
        let tracers = read_all(&dir_of(d)).unwrap_or_else(|e| panic!("{e}"));
        assert!(!tracers.is_empty(), "{id}");
        for (file, _, t) in &tracers {
            assert_eq!(t.kind, kind, "{}", file.display());
            assert_eq!(t.position.frame, d.frame, "{}", file.display());
        }
        assert_eq!(normalized(id).entries.len(), tracers.len(), "{id}");
    }
}

#[test]
fn a_parallax_is_measured_and_a_distance_resting_on_a_rotation_curve_is_inferred() {
    for (id, _) in SOURCES {
        let d = definition(id);
        for (file, _, t) in read_all(&dir_of(d)).unwrap() {
            if let Some(p) = &t.parallax {
                assert_eq!(p.value.kind, KindOfValue::Measured, "{}", file.display());
                assert_eq!(p.distance_kind, DistanceKind::Parallax);
            }
            if let Some(distance) = &t.distance {
                assert_eq!(
                    distance.value.kind,
                    KindOfValue::Inferred,
                    "{}",
                    file.display()
                );
                let assumes = distance.value.assumes.as_deref().unwrap();
                assert!(!assumes.stated.trim().is_empty(), "{}", file.display());
            }
        }
    }
    // Sanna et al. 2017's maser keeps its parallax beside the distance its paper adopts.
    let masers = normalized("researched-arm-masers");
    let sanna = masers
        .entries
        .iter()
        .find(|e| e.designation.identifier.starts_with("sanna-2017-"))
        .unwrap();
    assert!(sanna.parallax.is_some());
    assert_eq!(sanna.distances.len(), 1);
    assert_eq!(sanna.distances[0].kind_of_value, KindOfValue::Inferred);
    // The entry keeps what the distance assumes, as the paper states it.
    let assumes = sanna.flags.get(ASSUMES_FLAG).unwrap();
    assert!(assumes.contains("rotation curve"), "{assumes}");
    // Dame & Thaddeus 2011's clouds give only a kinematic distance.
    for e in &normalized("researched-arm-clouds").entries {
        assert!(e.parallax.is_none());
        assert_eq!(e.distances[0].kind_of_value, KindOfValue::Inferred);
    }
}

#[test]
fn each_tracer_carries_its_arm_and_its_sources_memberships_name_every_structure_of_that_arm() {
    let structures = read_all_named(&structures_dir()).unwrap();
    let mut of_arm: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (i, s) in &structures {
        if let Some(StructureGeometry::LogSpiral(l)) = &s.geometry {
            of_arm.entry(l.arm.slug()).or_default().insert(i.as_str());
        }
    }
    let slugs: BTreeSet<&str> = Arm::ALL.iter().map(|a| a.slug()).collect();
    for (id, _) in SOURCES {
        let d = definition(id);
        // The flag's values are the arms, every one and no other.
        let values: BTreeSet<&str> = d.flags[ARM_FLAG]
            .values
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(values, slugs, "{id}");
        let m = &d.memberships[0];
        assert_eq!(m.column, ARM_FLAG);
        for e in &normalized(id).entries {
            let arm = e.flags.get(ARM_FLAG).unwrap();
            let named: BTreeSet<&str> = m.structures(arm).iter().map(String::as_str).collect();
            let wanted = &of_arm[arm];
            assert_eq!(&named, wanted, "{id}: {arm}");
        }
    }
}

/// Sanna et al. 2017's tracer file as written, `edit` applied to it.
fn edited(edit: impl Fn(String) -> String) -> Result<TracerFile, String> {
    let file = workspace().join("tracers/masers/sanna-2017-g007-47p00-05.yaml");
    let text = std::fs::read_to_string(file).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-tracer.yaml");
    std::fs::write(&path, edit(text)).unwrap();
    TracerFile::read(&path).map_err(|e| e.to_string())
}

#[test]
fn a_tracer_with_an_uncited_number_or_no_distance_is_refused_naming_it() {
    assert!(edited(|t| t).is_ok());
    let uncited = edited(|t| {
        let at = t.find("parallax:").unwrap();
        let cite = at + t[at..].find("  cite:").unwrap();
        let end = cite + t[cite..].find("  note:").unwrap();
        format!("{}{}", &t[..cite], &t[end..])
    })
    .unwrap_err();
    assert!(uncited.contains("test-tracer.yaml"), "{uncited}");
    assert!(uncited.contains("parallax"), "{uncited}");
    let neither = edited(|t| {
        let cut = |t: &str, key: &str| {
            let at = t.find(&format!("\n{key}:")).unwrap();
            let end = at + 1 + t[at + 1..].find("\nvelocity_lsr:").unwrap();
            format!("{}{}", &t[..at], &t[end..])
        };
        cut(&cut(&t, "distance"), "parallax")
    })
    .unwrap_err();
    assert!(
        neither.contains("parallax, a distance or both"),
        "{neither}"
    );
    let measured = edited(|t| t.replacen("kind: inferred", "kind: measured", 1)).unwrap_err();
    assert!(measured.contains("inferred"), "{measured}");
    let parallax_kind = edited(|t| {
        t.replacen(
            "distance_kind: secondary_indicator",
            "distance_kind: parallax",
            1,
        )
    })
    .unwrap_err();
    assert!(parallax_kind.contains("other than"), "{parallax_kind}");
    let velocity = edited(|t| {
        let at = t.find("velocity_lsr:").unwrap();
        let unit = at + t[at..].find("unit: kilometre_per_second").unwrap();
        format!(
            "{}unit: kiloparsec{}",
            &t[..unit],
            &t[unit + "unit: kilometre_per_second".len()..]
        )
    })
    .unwrap_err();
    assert!(velocity.contains("velocity_lsr.unit"), "{velocity}");
    let notes = edited(|t| {
        let at = t.find("\nnotes:").unwrap();
        format!("{}\nnotes: \" \"\n", &t[..at])
    })
    .unwrap_err();
    assert!(notes.contains("notes"), "{notes}");
}

#[test]
fn a_release_holds_the_tracers_as_galaxy_tracers_members_of_their_arms() {
    let mut f = fixture();
    for (id, _) in SOURCES {
        let n = normalized(id);
        record(f.cache.path(), id, &n.report);
        f.definitions.push(definition(id).clone());
        f.normalized.push(n.clone());
    }
    let release = build_next(&f);
    let db = loaded(&release.dir, &release.manifest);
    let terms = terms(&db);
    for (id, kind) in SOURCES {
        let rows: Vec<(i64, i64, i64)> = db
            .prepare(
                "SELECT o.kind, o.scale_domain, \
                 (SELECT count(*) FROM memberships m WHERE m.release = o.release \
                  AND m.object_id = o.id) \
                 FROM entries e JOIN objects o ON o.release = e.release AND o.id = e.object_id \
                 WHERE e.release = 1 AND e.source = (SELECT n FROM sources WHERE id = ?1)",
            )
            .unwrap()
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(rows.len(), normalized(id).entries.len(), "{id}");
        let name = serde_json::to_value(kind).unwrap();
        for (k, domain, members) in rows {
            assert_eq!(Some(terms[&k].as_str()), name.as_str(), "{id}");
            assert_eq!(terms[&domain], "galaxy_tracers", "{id}");
            assert!(members >= 2, "{id}: a fit and its extrapolation");
        }
    }
}

/// The structures and memberships of a fixture release with the arm tracers and Reid et al.
/// 2019's masers, taking only the researched structures of `domains`.
fn built_taking(domains: &[ScaleDomain]) -> (Vec<String>, Vec<(String, String)>) {
    let mut f = fixture_with(&["reid-2019-masers"]);
    f.structures_of = domains.to_vec();
    for (id, _) in SOURCES {
        let n = normalized(id);
        record(f.cache.path(), id, &n.report);
        f.definitions.push(definition(id).clone());
        f.normalized.push(n.clone());
    }
    let release = build_next(&f);
    let db = loaded(&release.dir, &release.manifest);
    let structures = db
        .prepare("SELECT id FROM structures WHERE release = 1 ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let memberships = db
        .prepare(
            "SELECT s.id, m.structure FROM memberships m JOIN sources s ON s.release = m.release \
             AND s.n = m.source WHERE m.release = 1",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (structures, memberships)
}

#[test]
fn a_build_taking_the_galaxy_tracers_structures_holds_the_arms_and_their_memberships_only() {
    let (structures, memberships) = built_taking(&[ScaleDomain::GalaxyTracers]);
    let mut arms: Vec<String> = read_all_named(&structures_dir())
        .unwrap()
        .into_iter()
        .filter(|(_, s)| s.scale_domain == ScaleDomain::GalaxyTracers)
        .map(|(id, _)| id)
        .collect();
    arms.sort();
    assert_eq!(structures, arms);
    assert!(structures.iter().any(|id| id.starts_with("sun-2024-")));
    // Each tracer source's memberships reach every structure of each arm its tracers name.
    for (id, _) in SOURCES {
        let m = &definition(id).memberships[0];
        let wanted: BTreeSet<&str> = normalized(id)
            .entries
            .iter()
            .flat_map(|e| m.structures(e.flags.get(ARM_FLAG).unwrap()))
            .map(String::as_str)
            .collect();
        let held: BTreeSet<&str> = memberships
            .iter()
            .filter(|(s, _)| s == id)
            .map(|(_, structure)| structure.as_str())
            .collect();
        assert_eq!(held, wanted, "{id}");
    }
    assert!(memberships.iter().any(|(s, _)| s == "reid-2019-masers"));
    assert!(memberships.iter().all(|(_, m)| structures.contains(m)));
}

#[test]
fn a_membership_naming_a_structure_the_build_leaves_out_is_left_out_not_refused() {
    // Taking only the cosmic web's structures, the arms that every membership names are left
    // out: the build completes, with no membership.
    let (structures, memberships) = built_taking(&[ScaleDomain::CosmicWeb]);
    assert!(!structures.iter().any(|id| id.contains("-arm")));
    assert_eq!(memberships, []);
}
