//! The OpenAPI document of `/v1` as last committed, `openapi.v1.json`, and the rule it keeps
//! (read-the-openapi-document): within an API version a change only adds. What a client of the
//! snapshot relies on must still hold in the document:
//!
//! - every route, method, parameter, response, schema and field it lists, unchanged, and every
//!   value of an enumeration, and every field it requires;
//! - no request it accepted may be refused: a request body gains no required field and no new
//!   constraint, and an operation gains no required parameter.
//!
//! A removal, a rename or a narrowed request belongs to a new version. Once nothing was taken
//! away, the snapshot must be the document, so that what was added is held from then on:
//! `UNIV_OPENAPI_SNAPSHOT=write cargo test -p univ-api openapi_snapshot` writes it, and refuses
//! to while anything is taken away.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::layers::validate::pointer;
use crate::openapi::tests::value as current;

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

/// Members whose own members are named by the API (fields, paths, schemas, statuses, media
/// types), so that none of those is ever prose.
const NAMES: &[&str] = &["properties", "paths", "schemas", "responses", "content"];

/// Keywords that narrow what a value may be: added to a request, they refuse what it accepted.
const CONSTRAINTS: &[&str] = &[
    "maxLength",
    "minLength",
    "maximum",
    "minimum",
    "exclusiveMaximum",
    "exclusiveMinimum",
    "pattern",
    "maxItems",
    "minItems",
    "multipleOf",
    "additionalProperties",
    "const",
];

/// Each place where `new` lost or changed what `old` holds, by its JSON pointer; `named` when
/// the members of `old` are names the API gives.
fn taken_away(old: &Value, new: &Value, at: &str, named: bool) -> Vec<String> {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => old
            .iter()
            .filter(|(key, _)| named || !PROSE.contains(&key.as_str()))
            .flat_map(|(key, value)| {
                let here = pointer(at, key);
                match new.get(key) {
                    Some(now) => taken_away(value, now, &here, NAMES.contains(&key.as_str())),
                    None => vec![format!("{here}: removed")],
                }
            })
            .collect(),
        // A list keeps each member it had: a value of an enumeration, a required field, a
        // parameter, a variant.
        (Value::Array(old), Value::Array(new)) => old
            .iter()
            .enumerate()
            .filter(|(_, member)| {
                !new.iter()
                    .any(|now| taken_away(member, now, "", false).is_empty())
            })
            .map(|(i, member)| format!("{at}/{i}: {member} is no longer listed"))
            .collect(),
        (old, new) if old != new => vec![format!("{at}: was {old}, is {new}")],
        _ => Vec::new(),
    }
}

/// The names of the schemas a request body refers to, directly or through others.
fn request_schemas(document: &Value) -> BTreeSet<String> {
    let refs = |value: &Value| -> Vec<String> {
        let text = value.to_string();
        text.split("\"#/components/schemas/")
            .skip(1)
            .filter_map(|rest| rest.split('"').next().map(str::to_owned))
            .collect()
    };
    let mut names: Vec<String> = document["paths"]
        .as_object()
        .into_iter()
        .flat_map(|paths| paths.values())
        .filter_map(Value::as_object)
        .flat_map(|item| item.values())
        .flat_map(|operation| refs(&operation["requestBody"]))
        .collect();
    let mut found = BTreeSet::new();
    while let Some(name) = names.pop() {
        if found.insert(name.clone()) {
            names.extend(refs(&document["components"]["schemas"][&name]));
        }
    }
    found
}

/// Each place where a request schema of `new` refuses what `old` accepted: a field newly
/// required, or a constraint newly added.
fn narrowed(old: &Value, new: &Value, at: &str) -> Vec<String> {
    let (Value::Object(old), Value::Object(new)) = (old, new) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (key, now) in new {
        let here = pointer(at, key);
        match old.get(key) {
            None if CONSTRAINTS.contains(&key.as_str()) => {
                found.push(format!("{here}: a constraint added to a request"));
            }
            None if key == "required" => found.push(format!("{here}: fields newly required")),
            Some(Value::Array(was)) if key == "required" => {
                for field in now
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|f| !was.contains(f))
                {
                    found.push(format!("{here}: {field} newly required"));
                }
            }
            Some(was) => found.extend(narrowed(was, now, &here)),
            None => {}
        }
    }
    found
}

/// Each operation of `new` that requires a parameter `old`'s did not take.
fn parameters_required(old: &Value, new: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for (path, item) in new["paths"].as_object().into_iter().flatten() {
        for (method, operation) in item.as_object().into_iter().flatten() {
            let before = &old["paths"][path][method]["parameters"];
            let had = |name: &Value| {
                before
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|p| p["name"] == *name)
            };
            for parameter in operation["parameters"].as_array().into_iter().flatten() {
                if parameter["required"] == true && !had(&parameter["name"]) {
                    found.push(format!(
                        "{} {path}: parameter {} newly required",
                        method.to_uppercase(),
                        parameter["name"]
                    ));
                }
            }
        }
    }
    found
}

/// Everything `new` takes away from clients of `old`.
fn breaks(old: &Value, new: &Value) -> Vec<String> {
    let mut found = taken_away(old, new, "", false);
    for name in request_schemas(new) {
        let at = pointer("/components/schemas", &name);
        let was = &old["components"]["schemas"][&name];
        if !was.is_null() {
            found.extend(narrowed(was, &new["components"]["schemas"][&name], &at));
        }
    }
    found.extend(parameters_required(old, new));
    found
}

/// The snapshot; an empty document where there is none yet.
fn snapshot() -> Value {
    std::fs::read_to_string(SNAPSHOT)
        .map_or_else(|_| json!({}), |text| serde_json::from_str(&text).unwrap())
}

#[test]
fn within_v1_a_change_adds_to_the_document_and_removes_or_renames_nothing() {
    let now = current();
    let lost = breaks(&snapshot(), &now);
    assert!(
        lost.is_empty(),
        "the document takes away what clients of /v1 rely on; such a change belongs to a new \
         API version:\n{}",
        lost.join("\n")
    );
    if std::env::var("UNIV_OPENAPI_SNAPSHOT").as_deref() == Ok("write") {
        std::fs::write(SNAPSHOT, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
    } else {
        assert!(
            snapshot() == now,
            "the document added to the snapshot: write it again with \
             UNIV_OPENAPI_SNAPSHOT=write cargo test -p univ-api openapi_snapshot"
        );
    }
}

/// The members of schema `name` of `document`.
fn fields<'a>(document: &'a mut Value, name: &str) -> &'a mut serde_json::Map<String, Value> {
    document
        .pointer_mut(&format!("/components/schemas/{name}/properties"))
        .and_then(Value::as_object_mut)
        .unwrap()
}

#[test]
fn removing_renaming_or_narrowing_is_caught_and_adding_is_not() {
    type Change = fn(&mut Value);
    let old = current();
    let cases: [(&str, bool, Change); 9] = [
        ("a field removed", true, |d| {
            fields(d, "VolumePage").remove("served");
        }),
        ("a field named like prose removed", true, |d| {
            fields(d, "LayerRecord").remove("title");
        }),
        ("a field renamed", true, |d| {
            let served = fields(d, "VolumePage").remove("served").unwrap();
            fields(d, "VolumePage").insert("count".to_owned(), served);
        }),
        ("a route removed", true, |d| {
            d["paths"]
                .as_object_mut()
                .unwrap()
                .remove("/v1/query/volume");
        }),
        ("a value of an enumeration removed", true, |d| {
            let values = d
                .pointer_mut("/components/schemas/Visibility/enum")
                .unwrap();
            values.as_array_mut().unwrap().retain(|v| v != "private");
        }),
        ("a request field newly required", true, |d| {
            let required = d
                .pointer_mut("/components/schemas/CreateLayer/required")
                .unwrap();
            required.as_array_mut().unwrap().push("description".into());
        }),
        ("a constraint added to a request", true, |d| {
            fields(d, "CreateLayer")["title"]["maxLength"] = json!(10);
        }),
        ("a response field no longer required", true, |d| {
            let required = d
                .pointer_mut("/components/schemas/VolumePage/required")
                .unwrap();
            required.as_array_mut().unwrap().retain(|v| v != "served");
        }),
        (
            "a field and a value added, a description reworded",
            false,
            |d| {
                fields(d, "VolumePage").insert("note".to_owned(), json!({"type": "string"}));
                let required = d
                    .pointer_mut("/components/schemas/VolumePage/required")
                    .unwrap();
                required.as_array_mut().unwrap().push("note".into());
                let values = d
                    .pointer_mut("/components/schemas/Visibility/enum")
                    .unwrap();
                values.as_array_mut().unwrap().push("unlisted".into());
                if let Some(v) = d.pointer_mut("/components/schemas/VolumePage/description") {
                    *v = "Reworded.".into();
                }
            },
        ),
    ];
    for (case, caught, change) in cases {
        let mut new = old.clone();
        change(&mut new);
        assert_eq!(!breaks(&old, &new).is_empty(), caught, "{case}");
    }
}
