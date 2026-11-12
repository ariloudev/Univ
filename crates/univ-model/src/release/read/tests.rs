use serde_json::{Value, json};

use super::*;
use crate::api::objects::MeasuredQuantity;
use crate::object::{ObjectKind, Resolution};
use crate::units::Unit;
use crate::value::Uncertainty;

const TERMS: [&str; 16] = [
    "body",
    "anchor",
    "icrs",
    "degree",
    "velocity",
    "measured",
    "kilometre_per_second",
    "symmetric",
    "no_distance",
    "none_reported",
    "match",
    "position",
    "survey",
    "member_of",
    "q_Dist",
    "Ref",
];

fn id(term: &str) -> i64 {
    i64::try_from(TERMS.iter().position(|t| *t == term).unwrap()).unwrap()
}

fn vocabulary() -> Vocabulary {
    Vocabulary::new(
        7,
        Cosmology::planck_2018(),
        "the-rule".to_owned(),
        (0..).zip(TERMS.iter().map(|t| (*t).to_owned())),
        [(0, json!([{"step": "cartesian", "citation": "x"}]))],
        [(0, "sixdfgs-dr3".to_owned()), (1, "twomrs".to_owned())],
    )
}

fn row(value: Value) -> Row {
    let Value::Object(map) = value else {
        unreachable!()
    };
    map
}

/// An object not placed, as D1 gives it: some integers as whole floats (`kind` 0.0 is `body`).
fn object_row() -> Row {
    let mut columns = row(json!({"release": 7, "id": 42.0, "kind": 0.0,
        "ra": 10.5, "dec": -3.25, "epoch": 2000.0, "position_entry": 0,
        "position_class": id("survey"), "position_uncertainty_mas": null, "placed": 0,
        "not_placed": id("no_distance")}));
    for c in [
        "preferred_quantity",
        "preferred_entry",
        "preferred_n",
        "runner_up_quantity",
    ] {
        columns.insert(c.into(), Value::Null);
    }
    columns
}

fn entry_row() -> Row {
    row(
        json!({"entry": 0.0, "source": 1, "identifier": "J0042", "resolution": id("anchor"),
        "scheme": null, "shared": null, "via_entry": null, "confidence": null,
        "separation_arcsec": null, "difference": null, "delta": null, "name": null,
        "frame": id("icrs"), "lon": 10.5, "lat": -3.25, "position_unit": id("degree"),
        "epoch": 2000.0, "ra": null, "dec": null, "error_ra": null, "error_dec": null,
        "error_unit": null, "error_kind": null, "group_id": null, "via_source": null,
        "via_identifier": null, "spectral_type": null, "spectral_type_kind": null}),
    )
}

fn measurement_row() -> Row {
    row(json!({"entry": 0, "quantity": id("velocity"), "n": 0,
        "kind_of_value": id("measured"), "native_value": -300.0,
        "native_unit": id("kilometre_per_second"), "native_form": id("symmetric"),
        "native_minus": 4.0, "native_plus": 4.0, "native_level": null, "value": null,
        "unit": null, "form": null, "minus": null, "plus": null, "level": null, "kind": null,
        "method": null, "measure": null, "cmb": null, "cmb_form": null, "cmb_minus": null,
        "cmb_plus": null, "cmb_level": null, "published_frame": null}))
}

/// The object of `o` with its `entries`, `measurements` and `designations` rows and no others.
fn read_rows(
    v: &Vocabulary,
    o: &Row,
    entries: &[Row],
    measurements: &[Row],
    designations: &[Row],
) -> Result<ObjectRecord, ReadError> {
    object(
        v,
        &ObjectRows {
            object: o,
            entries,
            measurements,
            measurement_details: &[],
            entry_flags: &[],
            entry_references: &[],
            links: &[],
            relations: &[],
            designations,
            names: &[],
        },
    )
}

#[test]
fn a_vocabulary_names_its_terms_sources_and_chains() {
    let v = vocabulary();
    assert_eq!(v.release(), 7);
    assert_eq!(v.term(id("velocity")), Some("velocity"));
    assert_eq!(v.term(99), None);
    assert_eq!(v.source(1), Some("twomrs"));
    assert_eq!(v.source(2), None);
    assert!(v.chain(0).is_some());
    assert_eq!(v.cosmology(), &Cosmology::planck_2018());
}

#[test]
fn an_object_not_placed_reads_with_its_sky_position_and_reason() {
    let v = vocabulary();
    let read = read_rows(&v, &object_row(), &[entry_row()], &[measurement_row()], &[]).unwrap();
    assert_eq!(
        (read.release, read.id, read.kind),
        (7, 42, ObjectKind::Body)
    );
    assert_eq!(read.entries.len(), 1);
    let entry = &read.entries[0];
    assert_eq!(
        (entry.source.as_str(), entry.identifier.as_str()),
        ("twomrs", "J0042")
    );
    assert_eq!(entry.resolved_by, Resolution::Anchor);
    let position = entry.position.as_ref().unwrap();
    assert_eq!(
        (position.ra, position.dec),
        (10.5, -3.25),
        "ICRS degrees as published"
    );
    let velocity = &read.measurements[0];
    assert_eq!(velocity.quantity, MeasuredQuantity::Velocity);
    assert_eq!(velocity.source, "twomrs");
    assert_eq!(velocity.native.unit, Unit::KilometrePerSecond);
    assert_eq!(
        velocity.native.uncertainty,
        Uncertainty::Symmetric { sigma: 4.0 }
    );
    assert_eq!(velocity.normalized, None);
    assert!(read.position.is_none());
    assert_eq!(
        read.not_placed,
        Some(crate::object::NotPlacedReason::NoDistance)
    );
    let sky = read.sky_position.unwrap();
    assert_eq!(
        (sky.ra, sky.dec, sky.epoch, sky.entry),
        (10.5, -3.25, 2000.0, 0)
    );
}

#[test]
fn an_unknown_term_names_its_table_column_and_id() {
    let v = vocabulary();
    let mut o = object_row();
    o.insert("kind".into(), json!(99));
    let e = read_rows(&v, &o, &[entry_row()], &[], &[]).unwrap_err();
    assert!(e.0.contains("objects.kind"), "{e}");
    assert!(e.0.contains("term 99"), "{e}");
}

#[test]
fn a_missing_column_names_its_table_and_column() {
    let v = vocabulary();
    let mut e = entry_row();
    e.remove("identifier");
    let err = read_rows(&v, &object_row(), &[e], &[], &[]).unwrap_err();
    assert!(err.to_string().contains("entries.identifier"), "{err}");
    assert!(err.to_string().contains("no such column"), "{err}");
}

#[test]
fn an_integer_column_refuses_a_fraction_and_an_unknown_source() {
    let v = vocabulary();
    let mut m = measurement_row();
    m.insert("n".into(), json!(0.5));
    let err = read_rows(&v, &object_row(), &[entry_row()], &[m], &[]).unwrap_err();
    assert!(err.0.contains("measurements.n"), "{err}");
    let mut e = entry_row();
    e.insert("source".into(), json!(5));
    let err = read_rows(&v, &object_row(), &[e], &[], &[]).unwrap_err();
    assert!(err.0.contains("entries.source: 5"), "{err}");
}

#[test]
fn an_entrys_flags_and_papers_and_an_objects_links_and_relations_read_from_their_rows() {
    let v = vocabulary();
    let flags = [
        row(json!({"entry": 0, "flag": id("q_Dist"), "value": "B"})),
        row(json!({"entry": 0, "flag": id("Ref"), "value": "2009ApJS"})),
    ];
    let papers = [
        row(json!({"entry": 0, "n": 1, "citation": "Second 2020", "doi": null, "bibcode": null})),
        row(
            json!({"entry": 0, "n": 0, "citation": "First 2019", "doi": "10.1/x", "bibcode": null}),
        ),
    ];
    let links = [row(
        json!({"n": 0, "entry_source": 1, "entry_identifier": "J0042",
        "candidate_source": 0, "candidate_identifier": "g1", "resolution": id("match"),
        "scheme": id("position"), "shared": null, "via_source": 0, "via_identifier": "g1",
        "separation_arcsec": 3.5, "difference": null, "delta": null, "reason_confidence": 0.4,
        "confidence": 0.4}),
    )];
    let relations = [row(
        json!({"n": 0, "kind": id("member_of"), "entry_source": 1,
        "entry_identifier": "J0042", "other_source": 0, "other_identifier": "g1",
        "scheme": "group", "identifier": "7"}),
    )];
    let record = object(
        &v,
        &ObjectRows {
            object: &object_row(),
            entries: &[entry_row()],
            measurements: &[],
            measurement_details: &[],
            entry_flags: &flags,
            entry_references: &papers,
            links: &links,
            relations: &relations,
            designations: &[],
            names: &[],
        },
    )
    .unwrap();
    let entry = &record.entries[0];
    assert_eq!(entry.flags.get("Ref").map(String::as_str), Some("2009ApJS"));
    assert_eq!(entry.flags.len(), 2);
    let cited: Vec<&str> = entry
        .references
        .iter()
        .map(|p| p.citation.as_str())
        .collect();
    assert_eq!(cited, ["First 2019", "Second 2020"]);
    let Resolution::Match(m) = &record.links[0].reason else {
        panic!("{:?}", record.links[0].reason)
    };
    assert_eq!(m.method, crate::object::MatchMethod::Position);
    assert_eq!(&*m.with.identifier, "g1");
    assert_eq!(record.links[0].confidence.get(), 0.4);
    assert_eq!(&*record.links[0].candidate.identifier, "g1");
    assert_eq!(
        record.relations[0].kind,
        crate::object::RelationKind::MemberOf
    );
    assert_eq!(record.relations[0].identifier, "7");
}

fn designation_row(source: i64, identifier: &str) -> Row {
    row(json!({"source": source, "identifier": identifier}))
}

#[test]
fn the_designations_are_the_objects_rows_or_its_entries_own_before_that_table() {
    let v = vocabulary();
    let rows = [designation_row(1, "J0042"), designation_row(0, "g1")];
    let read = read_rows(&v, &object_row(), &[entry_row()], &[], &rows).unwrap();
    let named: Vec<(&str, &str)> = read
        .designations
        .iter()
        .map(|d| (d.source.as_str(), &*d.identifier))
        .collect();
    assert_eq!(named, [("sixdfgs-dr3", "g1"), ("twomrs", "J0042")]);
    let before = read_rows(&v, &object_row(), &[entry_row()], &[], &[]).unwrap();
    assert_eq!(before.designations.len(), 1);
    assert_eq!(&*before.designations[0].identifier, "J0042");
    let bad = [designation_row(9, "x")];
    let err = read_rows(&v, &object_row(), &[entry_row()], &[], &bad).unwrap_err();
    assert!(err.0.contains("designations.source"), "{err}");
}

#[test]
fn an_entry_numbered_twice_names_its_table_and_column() {
    let v = vocabulary();
    let err = read_rows(&v, &object_row(), &[entry_row(), entry_row()], &[], &[]).unwrap_err();
    assert!(
        err.0.contains("release 7 entries.entry: 0 appears twice"),
        "{err}"
    );
}
