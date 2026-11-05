//! A volume query's body (query-a-bounded-volume, filter-by-time). Its members are checked one
//! by one, so that every member that fails is named by its JSON pointer in one 400 answer;
//! the body that passes is then read as univ-model's [`VolumeQuery`], the type the OpenAPI
//! document describes, which gives the values.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use univ_model::api::query::{Filters, VolumeQuery};
use univ_model::encoding::hex;
use univ_model::position::Frame;
use univ_model::time::{TimeKind, Timescale};
use univ_model::units::Unit;
use univ_model::volume::{FRAME_RULE, Region, UNIT_RULE, VolumeError, check_frame, check_unit};

use crate::http::Request;
use crate::layers::broken;
use crate::layers::validate::{Failures, at, member, object, pointer, unknown};
use crate::problem::{Problem, ProblemType};

/// The most values one filter names.
const MAX_FILTER_VALUES: usize = 100;

/// A volume query's body, read.
pub(super) struct Body {
    /// The volume's region in the ICRS, in megaparsecs.
    pub region: Region,
    pub filters: Filters,
    /// The kind of time record compared, and the interval on TT, days from J2000.0.
    pub time: Option<(TimeKind, f64, f64)>,
    pub max_level: Option<u8>,
    pub cursor: Option<String>,
    /// What a cursor records of the query it pages: a digest of its volume, filters and time.
    pub fingerprint: String,
}

/// The body of `request` as a volume query.
///
/// # Errors
///
/// A 400 naming each member that fails by its JSON pointer.
pub(super) fn read(request: &Request) -> Result<Body, Problem> {
    let members: Map<String, Value> = request.json()?;
    let mut failures = Failures::default();
    let names = ["volume", "filters", "time", "tiles", "cursor"];
    unknown(&members, &names, "", "a volume query", &mut failures);
    volume(members.get("volume"), &mut failures);
    if let Some(value) = members.get("filters") {
        filters(value, &mut failures);
    }
    if let Some(value) = members.get("time") {
        time(value, &mut failures);
    }
    if let Some(value) = members.get("tiles")
        && let Some(tiles) = object(Some(value), "/tiles", "the tiles wanted", &mut failures)
    {
        unknown(
            tiles,
            &["max_level"],
            "/tiles",
            "the tiles wanted",
            &mut failures,
        );
        member::<u8>(tiles, "/tiles", "max_level", false, &mut failures);
    }
    member::<String>(&members, "", "cursor", false, &mut failures);
    invalid(failures)?;
    let digest = Sha256::digest(
        Value::Object(
            members
                .iter()
                .filter(|(name, _)| !["cursor", "tiles"].contains(&name.as_str()))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
        )
        .to_string(),
    );
    let query: VolumeQuery = serde_json::from_value(Value::Object(members))
        .map_err(|e| broken("A volume query that passed its checks", e))?;
    let region = query
        .volume
        .region_in(Unit::Megaparsec)
        .ok_or_else(|| broken("A volume that passed its checks", "it encloses no region"))?;
    let time = match query.time {
        None => None,
        Some(t) => {
            let bound = |b: &univ_model::api::query::TimeBound| b.at.on_tt(b.timescale);
            let (from, to) = (bound(&t.from), bound(&t.to));
            Some((
                t.kind,
                from.map_err(|e| broken("A bound", e))?,
                to.map_err(|e| broken("A bound", e))?,
            ))
        }
    };
    Ok(Body {
        region,
        filters: query.filters,
        time,
        max_level: query.tiles.and_then(|t| t.max_level),
        cursor: query.cursor,
        fingerprint: hex(&digest[..8]),
    })
}

/// The 400 that lists `failures`, if any.
pub(super) fn invalid(failures: Failures) -> Result<(), Problem> {
    failures.refuse(
        ProblemType::QueryInvalid,
        "The query failed the checks its errors name.",
    )
}

/// Member `name` of the volume read as a `T` that `rule` admits, or a failure with `reason`.
fn known<T: DeserializeOwned + Copy>(
    members: &Map<String, Value>,
    name: &str,
    rule: fn(T) -> Option<VolumeError>,
    reason: &str,
    failures: &mut Failures,
) -> Option<T> {
    let Some(value) = members.get(name) else {
        failures.add(pointer("/volume", name), "is required");
        return None;
    };
    let read = serde_json::from_value::<T>(value.clone())
        .ok()
        .filter(|v| rule(*v).is_none());
    if read.is_none() {
        failures.add(pointer("/volume", name), reason);
    }
    read
}

/// Checks the volume: its frame, its unit, and each member of its form.
fn volume(value: Option<&Value>, failures: &mut Failures) {
    let at_volume = "/volume";
    let Some(members) = object(value, at_volume, "a volume", failures) else {
        return;
    };
    known::<Frame>(members, "frame", check_frame, FRAME_RULE, failures);
    known::<Unit>(members, "unit", check_unit, UNIT_RULE, failures);
    let Some(named) = member::<String>(members, at_volume, "form", true, failures) else {
        return;
    };
    let own: &[&str] = match named.as_str() {
        "box" => &["min", "max"],
        "sphere" => &["centre", "radius"],
        "cone" => &["lon", "lat", "opening_angle", "near", "far"],
        _ => {
            failures.add("/volume/form", "is box, sphere or cone");
            return;
        }
    };
    let allowed: Vec<&str> = ["frame", "unit", "form"]
        .iter()
        .chain(own)
        .copied()
        .collect();
    unknown(
        members,
        &allowed,
        at_volume,
        &format!("a {named}"),
        failures,
    );
    let before = failures.len();
    for name in own {
        if ["min", "max", "centre"].contains(name) {
            member::<[f64; 3]>(members, at_volume, name, true, failures);
        } else {
            member::<f64>(members, at_volume, name, true, failures);
        }
    }
    if failures.len() > before {
        return;
    }
    // The form's members read: its own rules, from the form as univ-model reads it.
    let mut form = members.clone();
    form.retain(|name, _| name == "form" || own.contains(&name.as_str()));
    if let Ok(form) = serde_json::from_value::<univ_model::volume::VolumeForm>(Value::Object(form))
    {
        for error in form.check() {
            failures.add(pointer(at_volume, error.member), error.reason);
        }
    }
}

fn filters(value: &Value, failures: &mut Failures) {
    let Some(members) = object(Some(value), "/filters", "the filters", failures) else {
        return;
    };
    let names = ["kinds", "sources", "scale_domains", "layers"];
    unknown(members, &names, "/filters", "the filters", failures);
    for name in names {
        let values = member::<Vec<String>>(members, "/filters", name, false, failures);
        if values.is_some_and(|v| v.is_empty() || v.len() > MAX_FILTER_VALUES) {
            failures.add(
                pointer("/filters", name),
                format!("names 1 to {MAX_FILTER_VALUES} values"),
            );
        }
    }
}

fn time(value: &Value, failures: &mut Failures) {
    let Some(members) = object(Some(value), "/time", "a time filter", failures) else {
        return;
    };
    unknown(
        members,
        &["kind", "from", "to"],
        "/time",
        "a time filter",
        failures,
    );
    let kind = member::<TimeKind>(members, "/time", "kind", true, failures);
    let from = bound(members.get("from"), "/time/from", failures);
    let to = bound(members.get("to"), "/time/to", failures);
    if let (Some(_), Some(from), Some(to)) = (kind, from, to)
        && to < from
    {
        failures.add(
            "/time/to",
            "ends before from: an interval ends no earlier than it starts",
        );
    }
}

/// A bound of the interval on TT, days from J2000.0.
fn bound(value: Option<&Value>, base: &str, failures: &mut Failures) -> Option<f64> {
    let members = object(value, base, "a bound of time", failures)?;
    let names = ["timescale", "instant", "years_before_j2000"];
    unknown(members, &names, base, "a bound of time", failures);
    let before = failures.len();
    let timescale = member::<Timescale>(members, base, "timescale", true, failures);
    let when = at(
        members,
        base,
        "a bound of time",
        failures.len() == before,
        failures,
    )?;
    when.on_tt(timescale?)
        .map_err(|error| failures.add(pointer(base, error.member()), error.to_string()))
        .ok()
}
