//! The checks an item's content passes before it is stored, as published or revised
//! (publish-a-layer-item, revise-or-retract-an-item). Every failing member is named by its
//! JSON pointer with the reason, so a client mends them all at once.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use univ_model::api::layers::{ItemContent, ItemSource, Link};
use univ_model::geometry::Shape;
use univ_model::time::{OnTt, TimeKind, TimeRecord};
use univ_model::value::KindOfValue;

use crate::backend::Backend;
use crate::problem::{Problem, ProblemType};

/// The most time records, links or sources one item holds.
pub const MAX_ENTRIES: usize = 100;

/// The members of an item's content.
const MEMBERS: [&str; 7] = [
    "title",
    "summary",
    "geometry",
    "time_records",
    "kind_of_value",
    "links",
    "sources",
];

/// Content that passed every check, with its time records on TT.
pub(crate) struct Checked {
    pub content: ItemContent,
    pub times: Vec<(TimeKind, OnTt)>,
}

/// The failures found so far, each a JSON pointer and its reason.
#[derive(Default)]
pub(crate) struct Failures(Vec<(String, String)>);

impl Failures {
    pub(crate) fn add(&mut self, pointer: impl Into<String>, reason: impl Into<String>) {
        self.0.push((pointer.into(), reason.into()));
    }

    /// The refusal that lists them: 422, saying a source is required when none is cited.
    pub(crate) fn refusal(self, no_source: bool) -> Result<(), Problem> {
        if self.0.is_empty() {
            return Ok(());
        }
        let (kind, detail) = if no_source {
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
            .0
            .into_iter()
            .map(|(pointer, reason)| json!({"pointer": pointer, "reason": reason}))
            .collect();
        Err(Problem::new(kind, detail).with("errors", errors))
    }
}

/// Checks `value`, the content of an item at `base` in the request's body, against every rule
/// of an item, the links against every release's ids; failures go to `failures`. Gives the
/// content when nothing failed, and whether it cites no source.
pub(crate) async fn content<B: Backend>(
    backend: &B,
    value: Option<&Value>,
    base: &str,
    failures: &mut Failures,
) -> Result<(Option<Checked>, bool), Problem> {
    let Some(Value::Object(members)) = value else {
        failures.add(base, "an item's content is a JSON object");
        return Ok((None, false));
    };
    let before = failures.0.len();
    for name in members
        .keys()
        .filter(|name| !MEMBERS.contains(&name.as_str()))
    {
        failures.add(pointer(base, name), "is not a member of an item");
    }
    for name in ["title", "summary"] {
        match members.get(name) {
            Some(Value::String(text)) if !text.trim().is_empty() => {}
            _ => failures.add(
                pointer(base, name),
                "is required, as text that is not empty",
            ),
        }
    }
    read::<Shape>(members, base, "geometry", failures);
    read::<KindOfValue>(members, base, "kind_of_value", failures);
    let times = time_records(members, base, failures);
    links(backend, members, base, failures).await?;
    let no_source = sources(members, base, failures);
    if failures.0.len() > before {
        return Ok((None, no_source));
    }
    let content =
        serde_json::from_value::<ItemContent>(Value::Object(members.clone())).map_err(|error| {
            Problem::new(
                ProblemType::InternalError,
                format!("Content that passed its checks did not read: {error}."),
            )
        })?;
    Ok((Some(Checked { content, times }), no_source))
}

/// Member `name` read as a `T`, or a failure naming it.
fn read<T: DeserializeOwned>(
    members: &Map<String, Value>,
    base: &str,
    name: &str,
    failures: &mut Failures,
) -> Option<T> {
    let Some(value) = members.get(name) else {
        failures.add(pointer(base, name), "is required");
        return None;
    };
    serde_json::from_value(value.clone())
        .map_err(|error| failures.add(pointer(base, name), error.to_string()))
        .ok()
}

/// The entries of array member `name`, or a failure: absent reads as empty.
fn entries<'a>(
    members: &'a Map<String, Value>,
    base: &str,
    name: &str,
    failures: &mut Failures,
) -> &'a [Value] {
    match members.get(name) {
        None => &[],
        Some(Value::Array(entries)) if entries.len() <= MAX_ENTRIES => entries,
        Some(Value::Array(_)) => {
            failures.add(
                pointer(base, name),
                format!("holds at most {MAX_ENTRIES} entries"),
            );
            &[]
        }
        Some(_) => {
            failures.add(pointer(base, name), "is an array");
            &[]
        }
    }
}

fn time_records(
    members: &Map<String, Value>,
    base: &str,
    failures: &mut Failures,
) -> Vec<(TimeKind, OnTt)> {
    let at = pointer(base, "time_records");
    let mut times = Vec::new();
    for (i, entry) in entries(members, base, "time_records", failures)
        .iter()
        .enumerate()
    {
        let here = format!("{at}/{i}");
        match serde_json::from_value::<TimeRecord>(entry.clone()) {
            Err(error) => failures.add(here, error.to_string()),
            Ok(record) => match record.on_tt() {
                Ok(on) => times.push((record.kind, on)),
                Err(error) => failures.add(format!("{here}/{}", error.member()), error.to_string()),
            },
        }
    }
    times
}

async fn links<B: Backend>(
    backend: &B,
    members: &Map<String, Value>,
    base: &str,
    failures: &mut Failures,
) -> Result<(), Problem> {
    let at = pointer(base, "links");
    let mut targets = Vec::new();
    for (i, entry) in entries(members, base, "links", failures).iter().enumerate() {
        let here = format!("{at}/{i}");
        match serde_json::from_value::<Link>(entry.clone()) {
            Err(error) => failures.add(here, error.to_string()),
            Ok(Link {
                object: Some(id),
                structure: None,
                ..
            }) => targets.push((format!("{here}/object"), json!(id), true)),
            Ok(Link {
                object: None,
                structure: Some(id),
                ..
            }) => targets.push((format!("{here}/structure"), json!(id), false)),
            Ok(_) => failures.add(here, "a link names one object or one structure"),
        }
    }
    if targets.is_empty() {
        return Ok(());
    }
    let params: Vec<[Value; 1]> = targets.iter().map(|(_, id, _)| [id.clone()]).collect();
    let statements: Vec<(&str, &[Value])> = targets
        .iter()
        .zip(&params)
        .map(|((_, _, object), params)| {
            let sql = if *object {
                "SELECT 1 AS issued FROM ids_issued WHERE ?1 BETWEEN first AND last LIMIT 1"
            } else {
                "SELECT 1 AS issued FROM structures WHERE id = ?1 LIMIT 1"
            };
            (sql, params.as_slice())
        })
        .collect();
    let found = backend.releases_batch(&statements).await?;
    for ((here, id, object), rows) in targets.iter().zip(found) {
        if rows.is_empty() {
            let noun = if *object { "object" } else { "structure" };
            failures.add(
                here.clone(),
                format!("no release has published the {noun} {id}"),
            );
        }
    }
    Ok(())
}

/// Checks the sources; true when the item cites none.
fn sources(members: &Map<String, Value>, base: &str, failures: &mut Failures) -> bool {
    let at = pointer(base, "sources");
    let none = match members.get("sources") {
        None => true,
        Some(Value::Array(cited)) => cited.is_empty(),
        Some(_) => false,
    };
    if none {
        failures.add(at, "an item cites at least one source");
        return true;
    }
    for (i, entry) in entries(members, base, "sources", failures)
        .iter()
        .enumerate()
    {
        let here = format!("{at}/{i}");
        match serde_json::from_value::<ItemSource>(entry.clone()) {
            Err(error) => failures.add(here, error.to_string()),
            Ok(source) if source.title.trim().is_empty() => {
                failures.add(
                    format!("{here}/title"),
                    "is required, as text that is not empty",
                );
            }
            Ok(source)
                if source.bibcode.is_none() && source.doi.is_none() && source.url.is_none() =>
            {
                failures.add(here, "a source gives a bibcode, a DOI or a URL");
            }
            Ok(_) => {}
        }
    }
    false
}

/// The JSON pointer of member `name` under `base` (RFC 6901: `~` and `/` escaped).
pub(crate) fn pointer(base: &str, name: &str) -> String {
    format!("{base}/{}", name.replace('~', "~0").replace('/', "~1"))
}
