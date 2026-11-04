//! The checks every part of a request's body shares: a failure list by JSON pointer, members
//! read one by one, members no type holds refused.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::MAX_ENTRIES;
use crate::problem::{Problem, ProblemType};

/// The failures found, each a JSON pointer and its reason, and whether no source is cited.
#[derive(Default)]
pub(crate) struct Failures {
    list: Vec<(String, String)>,
    pub(crate) source_missing: bool,
}

impl Failures {
    /// Adds a failure; a pointer already named keeps its first reason.
    pub(crate) fn add(&mut self, pointer: impl Into<String>, reason: impl Into<String>) {
        let pointer = pointer.into();
        if !self.list.iter().any(|(named, _)| *named == pointer) {
            self.list.push((pointer, reason.into()));
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }

    /// The refusal that lists them all: 422, saying a source is required when none is cited.
    pub(crate) fn refusal(self) -> Result<(), Problem> {
        if self.list.is_empty() {
            return Ok(());
        }
        let (kind, detail) = if self.source_missing {
            (
                ProblemType::SourceRequired,
                "An item cites at least one source.",
            )
        } else {
            (
                ProblemType::ItemInvalid,
                "The item failed the checks its errors name.",
            )
        };
        let errors: Vec<Value> = self
            .list
            .into_iter()
            .map(|(pointer, reason)| json!({"pointer": pointer, "reason": reason}))
            .collect();
        Err(Problem::new(kind, detail).with("errors", errors))
    }
}

/// `value` as an object, or a failure saying what it should be.
pub(crate) fn object<'a>(
    value: Option<&'a Value>,
    at: &str,
    what: &str,
    failures: &mut Failures,
) -> Option<&'a Map<String, Value>> {
    match value {
        Some(Value::Object(members)) => Some(members),
        None => {
            failures.add(at, "is required");
            None
        }
        Some(_) => {
            failures.add(at, format!("{what} is a JSON object"));
            None
        }
    }
}

/// A failure for each member of `members` that is not one of `names`.
pub(crate) fn unknown(
    members: &Map<String, Value>,
    names: &[&str],
    at: &str,
    noun: &str,
    failures: &mut Failures,
) {
    for name in members
        .keys()
        .filter(|name| !names.contains(&name.as_str()))
    {
        failures.add(pointer(at, name), format!("is not a member of {noun}"));
    }
}

/// Member `name` read as a `T`: a failure where it does not read, or where it is required
/// and absent.
pub(crate) fn member<T: DeserializeOwned>(
    members: &Map<String, Value>,
    at: &str,
    name: &str,
    required: bool,
    failures: &mut Failures,
) -> Option<T> {
    match members.get(name) {
        None => {
            if required {
                failures.add(pointer(at, name), "is required");
            }
            None
        }
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| failures.add(pointer(at, name), error.to_string()))
            .ok(),
    }
}

/// Member `name` as text that is not empty, or a failure.
pub(crate) fn text(
    members: &Map<String, Value>,
    at: &str,
    name: &str,
    failures: &mut Failures,
) -> Option<String> {
    match members.get(name) {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        _ => {
            failures.add(pointer(at, name), "is required, as text that is not empty");
            None
        }
    }
}

/// The entries of array member `name`, or a failure: absent reads as empty.
pub(crate) fn entries<'a>(
    members: &'a Map<String, Value>,
    at: &str,
    name: &str,
    failures: &mut Failures,
) -> &'a [Value] {
    match members.get(name) {
        None => &[],
        Some(Value::Array(entries)) if entries.len() <= MAX_ENTRIES => entries,
        Some(Value::Array(_)) => {
            failures.add(
                pointer(at, name),
                format!("holds at most {MAX_ENTRIES} entries"),
            );
            &[]
        }
        Some(_) => {
            failures.add(pointer(at, name), "is an array");
            &[]
        }
    }
}

/// A failure for each member of `given` that the checked content `kept` does not hold: one
/// the item's types would otherwise drop, such as an unknown member of a geometry's kind.
pub(crate) fn dropped(given: &Value, kept: &Value, at: &str, failures: &mut Failures) {
    match (given, kept) {
        (Value::Object(given), Value::Object(kept)) => {
            for (name, value) in given {
                let here = pointer(at, name);
                match kept.get(name) {
                    Some(kept) => dropped(value, kept, &here, failures),
                    None if !value.is_null() => failures.add(here, "is not a member here"),
                    None => {}
                }
            }
        }
        (Value::Array(given), Value::Array(kept)) => {
            for (i, (value, kept)) in given.iter().zip(kept).enumerate() {
                dropped(value, kept, &format!("{at}/{i}"), failures);
            }
        }
        _ => {}
    }
}

/// The JSON pointer of the member serde reached at `path`, under `at`.
pub(crate) fn path(at: &str, path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    path.iter()
        .fold(at.to_owned(), |pointer, segment| match segment {
            Segment::Seq { index } => format!("{pointer}/{index}"),
            Segment::Map { key } => self::pointer(&pointer, key),
            Segment::Enum { .. } | Segment::Unknown => pointer,
        })
}

/// The JSON pointer of member `name` under `base` (RFC 6901: `~` and `/` escaped).
pub(crate) fn pointer(base: &str, name: &str) -> String {
    format!("{base}/{}", name.replace('~', "~0").replace('/', "~1"))
}
