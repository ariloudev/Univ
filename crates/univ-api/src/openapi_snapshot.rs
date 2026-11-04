//! The OpenAPI document of `/v1` as last committed, `openapi.v1.json`, and the rule it keeps
//! (read-the-openapi-document): within an API version a change only adds. A route, a method, a
//! parameter, a response, a schema, a field or a value of an enumeration that the snapshot holds
//! must still be in the document, unchanged; a removal or rename belongs to a new version.
//!
//! Once nothing was taken away, the snapshot must be the document, so that what was added is
//! held from then on: `UNIV_OPENAPI_SNAPSHOT=write cargo test -p univ-api openapi_snapshot`
//! writes it, and refuses to while anything is taken away.

use serde_json::Value;

use crate::openapi::document;

/// The snapshot, beside the crate's manifest.
const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.v1.json");

/// Members that describe and do not bind a client: they may change within a version.
const PROSE: &[&str] = &[
    "description",
    "summary",
    "title",
    "example",
    "examples",
    "info",
];

/// Each place where `new` lost or changed what `old` holds, by its JSON pointer.
fn taken_away(old: &Value, new: &Value, at: &str, out: &mut Vec<String>) {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            for (key, value) in old {
                if PROSE.contains(&key.as_str()) || key == "required" {
                    continue;
                }
                let here = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                match new.get(key) {
                    Some(now) => taken_away(value, now, &here, out),
                    None => out.push(format!("{here}: removed")),
                }
            }
        }
        // A list keeps each member it had: a value of an enumeration, a parameter, a variant.
        (Value::Array(old), Value::Array(new)) => {
            for (i, member) in old.iter().enumerate() {
                let kept = new.iter().any(|now| {
                    let mut lost = Vec::new();
                    taken_away(member, now, "", &mut lost);
                    lost.is_empty()
                });
                if !kept {
                    out.push(format!("{at}/{i}: {member} is no longer listed"));
                }
            }
        }
        (old, new) if old != new => out.push(format!("{at}: was {old}, is {new}")),
        _ => {}
    }
}

fn current() -> Value {
    serde_json::to_value(document()).unwrap()
}

fn snapshot() -> Value {
    serde_json::from_str(&std::fs::read_to_string(SNAPSHOT).unwrap()).unwrap()
}

#[test]
fn within_v1_a_change_adds_to_the_document_and_removes_or_renames_nothing() {
    let mut lost = Vec::new();
    taken_away(&snapshot(), &current(), "", &mut lost);
    assert!(
        lost.is_empty(),
        "the document lost what clients of /v1 rely on; such a change belongs to a new API \
         version:\n{}",
        lost.join("\n")
    );
    if std::env::var("UNIV_OPENAPI_SNAPSHOT").as_deref() == Ok("write") {
        let pretty = serde_json::to_string_pretty(&current()).unwrap();
        std::fs::write(SNAPSHOT, pretty + "\n").unwrap();
    }
    assert!(
        snapshot() == current(),
        "the document added to the snapshot: write it again with \
         UNIV_OPENAPI_SNAPSHOT=write cargo test -p univ-api openapi_snapshot"
    );
}

#[test]
fn removing_a_field_a_route_or_a_value_of_an_enumeration_is_caught_and_adding_one_is_not() {
    let old = current();
    let mut lost = Vec::new();
    let mut check = |change: &dyn Fn(&mut Value)| {
        let mut new = old.clone();
        change(&mut new);
        lost.clear();
        taken_away(&old, &new, "", &mut lost);
        lost.len()
    };
    let schemas = "/components/schemas";
    assert_eq!(
        check(&|d| {
            d.pointer_mut(&format!("{schemas}/VolumePage/properties"))
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("served");
        }),
        1,
        "a field removed"
    );
    assert_eq!(
        check(&|d| {
            let properties = d
                .pointer_mut(&format!("{schemas}/VolumePage/properties"))
                .unwrap()
                .as_object_mut()
                .unwrap();
            let served = properties.remove("served").unwrap();
            properties.insert("count".to_owned(), served);
        }),
        1,
        "a field renamed"
    );
    assert_eq!(
        check(&|d| {
            d["paths"]
                .as_object_mut()
                .unwrap()
                .remove("/v1/query/volume");
        }),
        1,
        "a route removed"
    );
    assert_eq!(
        check(&|d| {
            d.pointer_mut(&format!("{schemas}/Visibility/enum"))
                .unwrap()
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "private");
        }),
        1,
        "a value of an enumeration removed"
    );
    assert_eq!(
        check(&|d| {
            d.pointer_mut(&format!("{schemas}/VolumePage/properties"))
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("note".to_owned(), serde_json::json!({"type": "string"}));
            d.pointer_mut(&format!("{schemas}/Visibility/enum"))
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push("unlisted".into());
            if let Some(v) = d.pointer_mut(&format!("{schemas}/VolumePage/description")) {
                *v = "Reworded.".into();
            }
        }),
        0,
        "a field and a value added, a description reworded"
    );
}
