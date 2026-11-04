//! The checks an item's content passes before it is stored, as published or revised
//! (publish-a-layer-item, revise-or-retract-an-item). Each member of the item, of its
//! geometry, of each time record, link and source is checked on its own, and every failing
//! member is named by its JSON pointer (RFC 6901) with the reason, so a client mends them all
//! at once. A member the item's types do not hold is refused, never dropped.

mod fields;

pub(crate) use fields::{Failures, member, pointer, text, unknown};

use fields::{dropped, entries, object, path};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use univ_model::api::layers::{ItemContent, ItemSource, Link, Relation};
use univ_model::geometry::{Geometry, Shape};
use univ_model::position::Frame;
use univ_model::time::{At, OnTt, TimeKind, TimeRecord, TimeUncertainty, Timescale};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use crate::backend::{Backend, column};
use crate::problem::Problem;

/// The most time records, links or sources one item holds.
pub const MAX_ENTRIES: usize = 100;

/// Content that passed every check, with its time records on TT.
pub(crate) struct Checked {
    pub content: ItemContent,
    pub times: Vec<(TimeKind, OnTt)>,
}

/// Checks `value`, the content of an item at `base` in the request's body, against every rule
/// of an item, its links against every published release; failures go to `failures`. Gives
/// the content when none of its checks failed.
pub(crate) async fn content<B: Backend>(
    backend: &B,
    value: Option<&Value>,
    base: &str,
    failures: &mut Failures,
) -> Result<Option<Checked>, Problem> {
    let Some(members) = object(value, base, "an item's content", failures) else {
        return Ok(None);
    };
    let before = failures.len();
    let names = [
        "title",
        "summary",
        "geometry",
        "time_records",
        "kind_of_value",
        "links",
        "sources",
    ];
    unknown(members, &names, base, "an item", failures);
    let title = text(members, base, "title", failures);
    let summary = text(members, base, "summary", failures);
    let geometry = shape(
        members.get("geometry"),
        &pointer(base, "geometry"),
        failures,
    );
    let kind_of_value = member::<KindOfValue>(members, base, "kind_of_value", true, failures);
    let mut times = Vec::new();
    let mut records = Vec::new();
    let at = pointer(base, "time_records");
    for (i, entry) in entries(members, base, "time_records", failures)
        .iter()
        .enumerate()
    {
        if let Some((record, on)) = time_record(entry, &format!("{at}/{i}"), failures) {
            times.push((record.kind, on));
            records.push(record);
        }
    }
    let links = links(backend, members, base, failures).await?;
    let sources = sources(members, base, failures);
    if failures.len() > before {
        return Ok(None);
    }
    let (Some(title), Some(summary), Some(geometry), Some(kind_of_value)) =
        (title, summary, geometry, kind_of_value)
    else {
        return Ok(None);
    };
    let content = ItemContent {
        title,
        summary,
        geometry,
        time_records: records,
        kind_of_value,
        links,
        sources,
    };
    let kept = serde_json::to_value(&content).map_err(|e| super::broken("An item", e))?;
    dropped(value.unwrap_or(&Value::Null), &kept, base, failures);
    Ok((failures.len() == before).then_some(Checked { content, times }))
}

/// The geometry: a frame, a unit and a geometry of univ-model's kinds, then the shape's own
/// checks, such as a unit that is a length.
fn shape(value: Option<&Value>, at: &str, failures: &mut Failures) -> Option<Shape> {
    let members = object(value, at, "a geometry", failures)?;
    let before = failures.len();
    unknown(
        members,
        &["frame", "unit", "geometry"],
        at,
        "a geometry",
        failures,
    );
    member::<Frame>(members, at, "frame", true, failures);
    member::<Unit>(members, at, "unit", true, failures);
    let inner = pointer(at, "geometry");
    match members.get("geometry") {
        None => failures.add(&inner, "is required"),
        Some(geometry) => {
            if let Err(error) = serde_path_to_error::deserialize::<_, Geometry>(geometry) {
                failures.add(path(&inner, error.path()), error.inner().to_string());
            }
        }
    }
    if failures.len() > before {
        return None;
    }
    let shape = serde_json::from_value::<Shape>(Value::Object(members.clone()))
        .map_err(|error| failures.add(at, error.to_string()))
        .ok()?;
    let kept = serde_json::to_value(&shape).unwrap_or(Value::Null);
    dropped(value.unwrap_or(&Value::Null), &kept, at, failures);
    (failures.len() == before).then_some(shape)
}

/// A time record: its kind, its timescale, an instant or years before J2000.0 (not both),
/// and its uncertainty, then placed on TT.
fn time_record(value: &Value, at: &str, failures: &mut Failures) -> Option<(TimeRecord, OnTt)> {
    let members = object(Some(value), at, "a time record", failures)?;
    let before = failures.len();
    let names = [
        "kind",
        "timescale",
        "instant",
        "years_before_j2000",
        "uncertainty",
    ];
    unknown(members, &names, at, "a time record", failures);
    let kind = member::<TimeKind>(members, at, "kind", true, failures);
    let timescale = member::<Timescale>(members, at, "timescale", true, failures);
    let instant = member::<String>(members, at, "instant", false, failures);
    let years = member::<f64>(members, at, "years_before_j2000", false, failures);
    let when = match (instant, years) {
        (Some(instant), None) => Some(At::Instant { instant }),
        (None, Some(years_before_j2000)) => Some(At::YearsBefore { years_before_j2000 }),
        (Some(_), Some(_)) => {
            failures.add(
                pointer(at, "years_before_j2000"),
                "a time record gives an instant or years before J2000.0, not both",
            );
            None
        }
        (None, None) if failures.len() == before => {
            failures.add(at, "a time record gives an instant or years_before_j2000");
            None
        }
        (None, None) => None,
    };
    let uncertainty = match members.get("uncertainty") {
        None => {
            failures.add(pointer(at, "uncertainty"), "is required");
            None
        }
        Some(value) => serde_path_to_error::deserialize::<_, TimeUncertainty>(value)
            .map_err(|error| {
                let inner = pointer(at, "uncertainty");
                failures.add(path(&inner, error.path()), error.inner().to_string());
            })
            .ok(),
    };
    let record = TimeRecord {
        kind: kind?,
        timescale: timescale?,
        at: when?,
        uncertainty: uncertainty?,
    };
    match record.on_tt() {
        Ok(on) => Some((record, on)),
        Err(error) => {
            failures.add(pointer(at, error.member()), error.to_string());
            None
        }
    }
}

/// The links, each a relation and one object or one structure that a published release
/// holds; two queries check them all.
async fn links<B: Backend>(
    backend: &B,
    members: &Map<String, Value>,
    base: &str,
    failures: &mut Failures,
) -> Result<Vec<Link>, Problem> {
    let at = pointer(base, "links");
    let mut links = Vec::new();
    for (i, entry) in entries(members, base, "links", failures).iter().enumerate() {
        let here = format!("{at}/{i}");
        let Some(fields) = object(Some(entry), &here, "a link", failures) else {
            continue;
        };
        let before = failures.len();
        unknown(
            fields,
            &["relation", "object", "structure"],
            &here,
            "a link",
            failures,
        );
        let relation = member::<Relation>(fields, &here, "relation", true, failures);
        let object = member::<u64>(fields, &here, "object", false, failures);
        let structure = member::<String>(fields, &here, "structure", false, failures);
        if object.is_some() == structure.is_some() && failures.len() == before {
            failures.add(&here, "a link names one object or one structure");
        }
        if let (Some(relation), true) = (relation, failures.len() == before) {
            links.push((
                here,
                Link {
                    relation,
                    object,
                    structure,
                },
            ));
        }
    }
    let objects: Vec<u64> = links.iter().filter_map(|(_, link)| link.object).collect();
    let structures: Vec<&str> = links
        .iter()
        .filter_map(|(_, l)| l.structure.as_deref())
        .collect();
    if !links.is_empty() {
        let (objects, structures) = (json!(objects), json!(structures));
        let rows = backend
            .releases_batch(&[
                (
                    "SELECT value AS id FROM json_each(?1) WHERE NOT EXISTS (SELECT 1 \
                     FROM ids_issued d JOIN releases r ON r.number = d.release \
                     WHERE r.published_at IS NOT NULL AND value BETWEEN d.first AND d.last)",
                    &[objects],
                ),
                (
                    "SELECT value AS id FROM json_each(?1) WHERE NOT EXISTS (SELECT 1 \
                     FROM structures s JOIN releases r ON r.number = s.release \
                     WHERE r.published_at IS NOT NULL AND s.id = value)",
                    &[structures],
                ),
            ])
            .await?;
        let mut rows = rows.into_iter();
        let objects: Vec<u64> = ids(rows.next())?;
        let structures: Vec<String> = ids(rows.next())?;
        for (here, link) in &links {
            if let Some(id) = link.object.filter(|id| objects.contains(id)) {
                failures.add(
                    format!("{here}/object"),
                    format!("no release has published the object {id}"),
                );
            }
            if let Some(id) = link.structure.as_ref().filter(|id| structures.contains(id)) {
                failures.add(
                    format!("{here}/structure"),
                    format!("no release has published the structure {id:?}"),
                );
            }
        }
    }
    Ok(links.into_iter().map(|(_, link)| link).collect())
}

fn ids<T: DeserializeOwned>(rows: Option<Vec<crate::Row>>) -> Result<Vec<T>, Problem> {
    rows.unwrap_or_default()
        .iter()
        .map(|row| column(row, "id"))
        .collect()
}

/// The sources: at least one, each with a title and a bibcode, a DOI or a URL.
fn sources(members: &Map<String, Value>, base: &str, failures: &mut Failures) -> Vec<ItemSource> {
    let at = pointer(base, "sources");
    let none = match members.get("sources") {
        None => true,
        Some(Value::Array(cited)) => cited.is_empty(),
        Some(_) => false,
    };
    if none {
        failures.add(at, "an item cites at least one source");
        failures.source_missing = true;
        return Vec::new();
    }
    let mut sources = Vec::new();
    for (i, entry) in entries(members, base, "sources", failures)
        .iter()
        .enumerate()
    {
        let here = format!("{at}/{i}");
        let Some(fields) = object(Some(entry), &here, "a source", failures) else {
            continue;
        };
        let before = failures.len();
        unknown(
            fields,
            &["title", "bibcode", "doi", "url"],
            &here,
            "a source",
            failures,
        );
        let title = text(fields, &here, "title", failures);
        let mut id = |name| {
            let id = member::<String>(fields, &here, name, false, failures);
            if id.as_ref().is_some_and(|id| id.trim().is_empty()) {
                failures.add(pointer(&here, name), "is text that is not empty");
            }
            id
        };
        let (bibcode, doi, url) = (id("bibcode"), id("doi"), id("url"));
        if bibcode.is_none() && doi.is_none() && url.is_none() && failures.len() == before {
            failures.add(&here, "a source gives a bibcode, a DOI or a URL");
        }
        if let (Some(title), true) = (title, failures.len() == before) {
            sources.push(ItemSource {
                title,
                bibcode,
                doi,
                url,
            });
        }
    }
    sources
}
