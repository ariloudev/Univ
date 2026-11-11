//! A source of the Galaxy's tracers names the Galaxy tracers scale domain and the kind of object
//! its rows are; no other source names a kind (define-a-source, normalize-records).

mod common;

use common::{VALID_DEFINITION as VALID, read_definition as read, sources_dir};
use univ_ingest::source::{Access, ScaleDomain, read_all};
use univ_model::object::ObjectKind;

/// `VALID` as a source of the Galaxy's tracers, with `kind` as its kind of object, if any.
fn tracers(kind: Option<&str>) -> String {
    let domain = "scale_domain: galaxy_tracers";
    let named = kind.map_or(domain.to_owned(), |k| format!("{domain}\nobject_kind: {k}"));
    VALID.replace("scale_domain: cosmic_web", &named)
}

#[test]
fn a_tracer_source_names_the_galaxy_tracers_domain_and_its_kind_of_object() {
    for (name, kind) in [
        ("open_cluster", ObjectKind::OpenCluster),
        ("globular_cluster", ObjectKind::GlobularCluster),
        ("classical_cepheid", ObjectKind::ClassicalCepheid),
        ("rr_lyrae_star", ObjectKind::RrLyraeStar),
        ("star_forming_region", ObjectKind::StarFormingRegion),
        ("luminous_hot_star", ObjectKind::LuminousHotStar),
    ] {
        let definition = read(&tracers(Some(name))).unwrap();
        assert_eq!(definition.scale_domain, ScaleDomain::GalaxyTracers);
        assert_eq!(definition.object_kind, Some(kind));
        assert!(kind.is_tracer());
    }
    assert_eq!(read(VALID).unwrap().object_kind, None);
}

#[test]
fn a_tracer_source_without_a_kind_or_another_source_with_one_is_refused() {
    let missing = read(&tracers(None)).unwrap_err();
    assert_eq!(missing.field, "object_kind");
    assert!(missing.problem.contains("is missing"), "{missing}");
    let elsewhere = VALID.replace(
        "scale_domain: cosmic_web",
        "scale_domain: cosmic_web\nobject_kind: open_cluster",
    );
    assert_eq!(read(&elsewhere).unwrap_err().field, "object_kind");
    // A tracer source names a tracer's kind, not a body's or a group's.
    assert_eq!(
        read(&tracers(Some("galaxy_group"))).unwrap_err().field,
        "object_kind"
    );
}

#[test]
fn the_luminous_hot_stars_are_every_row_of_zari_2021s_filtered_sample() {
    let definitions = read_all(&sources_dir()).unwrap();
    let zari = definitions
        .iter()
        .find(|d| d.id.as_str() == "zari-2021-luminous-hot-stars")
        .unwrap();
    // Every row, as the ReadMe counts filtered.dat, with no cut.
    let Access::VizierTsv {
        table,
        constraints,
        rows,
        ..
    } = &zari.access
    else {
        unreachable!("{:?}", zari.access)
    };
    assert_eq!(table, "J/A+A/650/A112/filtered");
    assert_eq!(constraints, &Vec::<String>::new());
    assert_eq!(*rows, Some(417_535));
    assert_eq!(zari.scale_domain, ScaleDomain::GalaxyTracers);
    assert_eq!(zari.object_kind, Some(ObjectKind::LuminousHotStar));
}
