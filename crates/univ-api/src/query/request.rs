//! A volume query's body, read member by member so that every member that fails is named by
//! its JSON pointer in one 400 answer (query-a-bounded-volume, filter-by-time).

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use univ_model::api::query::Filters;
use univ_model::encoding::hex;
use univ_model::position::{Frame, Vec3};
use univ_model::time::{At, TimeKind, TimeRecord, TimeUncertainty, Timescale};
use univ_model::units::{Dimension, Unit};
use univ_model::volume::{Volume, VolumeForm};

use crate::http::Request;
use crate::layers::validate::{Failures, member, pointer, unknown};
use crate::problem::{Problem, ProblemType};

/// The most values one filter names.
const MAX_FILTER_VALUES: usize = 100;

/// A volume query's body, read.
pub(super) struct Body {
    pub volume: Volume,
    pub filters: Filters,
    /// The kind of time record compared, and the interval on TT, days from J2000.0.
    pub time: Option<(TimeKind, f64, f64)>,
    pub max_level: Option<u8>,
    pub cursor: Option<String>,
    /// What a cursor records of the query it pages: a digest of its body without the cursor.
    pub fingerprint: String,
}

/// The body of `request` as a volume query.
///
/// # Errors
///
/// A 400 naming each member that fails by its JSON pointer.
pub(super) fn read(request: &Request) -> Result<Body, Problem> {
    let mut members: Map<String, Value> = request.json()?;
    let mut failures = Failures::default();
    let names = ["volume", "filters", "time", "tiles", "cursor"];
    unknown(&members, &names, "", "a volume query", &mut failures);
    let volume = volume(members.get("volume"), &mut failures);
    let filters = filters(members.get("filters"), &mut failures);
    let time = members.get("time").and_then(|t| time(t, &mut failures));
    let max_level = members.get("tiles").and_then(|t| tiles(t, &mut failures));
    let cursor = member::<String>(&members, "", "cursor", false, &mut failures);
    invalid(failures)?;
    members.remove("cursor");
    let digest = Sha256::digest(Value::Object(members).to_string());
    let Some(volume) = volume else {
        return Err(Problem::new(
            ProblemType::QueryInvalid,
            "The volume did not read.",
        ));
    };
    Ok(Body {
        volume,
        filters,
        time,
        max_level,
        cursor,
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

/// The members `value` holds, or a failure at `at`.
fn object<'a>(
    value: Option<&'a Value>,
    at: &str,
    failures: &mut Failures,
) -> Option<&'a Map<String, Value>> {
    match value {
        Some(Value::Object(members)) => Some(members),
        None => {
            failures.add(at, "is required");
            None
        }
        Some(_) => {
            failures.add(at, "is a JSON object");
            None
        }
    }
}

fn volume(value: Option<&Value>, failures: &mut Failures) -> Option<Volume> {
    let at = "/volume";
    let members = object(value, at, failures)?;
    let frame = members.get("frame").and_then(|v| {
        let frame = serde_json::from_value::<Frame>(v.clone()).ok();
        let known =
            frame.filter(|f| matches!(f, Frame::Icrs | Frame::Galactic | Frame::Supergalactic));
        if known.is_none() {
            failures.add("/volume/frame", "is icrs, galactic or supergalactic");
        }
        known
    });
    if !members.contains_key("frame") {
        failures.add("/volume/frame", "is required");
    }
    let unit = members.get("unit").and_then(|v| {
        let unit = serde_json::from_value::<Unit>(v.clone()).ok();
        let length = unit.filter(|u| u.dimension() == Dimension::Length);
        if length.is_none() {
            failures.add(
                "/volume/unit",
                "is a unit of length, such as parsec or megaparsec",
            );
        }
        length
    });
    if !members.contains_key("unit") {
        failures.add("/volume/unit", "is required");
    }
    let form = form(members, failures);
    let volume = Volume {
        frame: frame?,
        unit: unit?,
        form: form?,
    };
    let errors = volume.check();
    for error in &errors {
        failures.add(pointer(at, error.member), error.reason.clone());
    }
    errors.is_empty().then_some(volume)
}

fn form(members: &Map<String, Value>, failures: &mut Failures) -> Option<VolumeForm> {
    let at = "/volume";
    let named = member::<String>(members, at, "form", true, failures)?;
    let own: &[&str] = match named.as_str() {
        "box" => &["min", "max"],
        "sphere" => &["centre", "radius"],
        "cone" => &["lon", "lat", "opening_angle", "near", "far"],
        _ => {
            failures.add("/volume/form", "is box, sphere or cone");
            return None;
        }
    };
    let allowed: Vec<&str> = ["frame", "unit", "form"]
        .iter()
        .chain(own)
        .copied()
        .collect();
    unknown(members, &allowed, at, &format!("a {named}"), failures);
    let point =
        |name: &str, failures: &mut Failures| member::<Vec3>(members, at, name, true, failures);
    let number =
        |name: &str, failures: &mut Failures| member::<f64>(members, at, name, true, failures);
    Some(match named.as_str() {
        "box" => {
            let (min, max) = (point("min", failures), point("max", failures));
            VolumeForm::Box {
                min: min?,
                max: max?,
            }
        }
        "sphere" => {
            let (centre, radius) = (point("centre", failures), number("radius", failures));
            VolumeForm::Sphere {
                centre: centre?,
                radius: radius?,
            }
        }
        _ => {
            let numbers: Vec<Option<f64>> = own.iter().map(|name| number(name, failures)).collect();
            VolumeForm::Cone {
                lon: numbers[0]?,
                lat: numbers[1]?,
                opening_angle: numbers[2]?,
                near: numbers[3]?,
                far: numbers[4]?,
            }
        }
    })
}

fn filters(value: Option<&Value>, failures: &mut Failures) -> Filters {
    let Some(value) = value else {
        return Filters::default();
    };
    let Some(members) = object(Some(value), "/filters", failures) else {
        return Filters::default();
    };
    let names = ["kinds", "sources", "scale_domains", "layers"];
    unknown(members, &names, "/filters", "the filters", failures);
    let mut list = |name: &str| {
        let values = member::<Vec<String>>(members, "/filters", name, false, failures)?;
        if values.is_empty() || values.len() > MAX_FILTER_VALUES {
            failures.add(
                pointer("/filters", name),
                format!("names 1 to {MAX_FILTER_VALUES} values"),
            );
            return None;
        }
        Some(values)
    };
    Filters {
        kinds: list("kinds"),
        sources: list("sources"),
        scale_domains: list("scale_domains"),
        layers: list("layers"),
    }
}

fn time(value: &Value, failures: &mut Failures) -> Option<(TimeKind, f64, f64)> {
    let members = object(Some(value), "/time", failures)?;
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
    let (kind, from, to) = (kind?, from?, to?);
    if to < from {
        failures.add(
            "/time/to",
            "ends before from: an interval ends no earlier than it starts",
        );
        return None;
    }
    Some((kind, from, to))
}

/// A bound of the interval on TT, days from J2000.0.
fn bound(value: Option<&Value>, at: &str, failures: &mut Failures) -> Option<f64> {
    let members = object(value, at, failures)?;
    let names = ["timescale", "instant", "years_before_j2000"];
    unknown(members, &names, at, "a bound of time", failures);
    let timescale = member::<Timescale>(members, at, "timescale", true, failures);
    let instant = member::<String>(members, at, "instant", false, failures);
    let years = member::<f64>(members, at, "years_before_j2000", false, failures);
    let at_time = match (instant, years) {
        (Some(instant), None) => At::Instant { instant },
        (None, Some(years_before_j2000)) => At::YearsBefore { years_before_j2000 },
        (Some(_), Some(_)) => {
            failures.add(
                pointer(at, "years_before_j2000"),
                "is given with an instant: a bound gives one or the other",
            );
            return None;
        }
        (None, None) => {
            if !members.contains_key("instant") && !members.contains_key("years_before_j2000") {
                failures.add(at, "gives an instant or years_before_j2000");
            }
            return None;
        }
    };
    let record = TimeRecord {
        kind: TimeKind::Arrival,
        timescale: timescale?,
        at: at_time,
        uncertainty: TimeUncertainty::NoneReported,
    };
    record
        .on_tt()
        .map(|on| on.at)
        .map_err(|error| failures.add(pointer(at, error.member()), error.to_string()))
        .ok()
}

/// The `tiles` member's deepest level, if any.
fn tiles(value: &Value, failures: &mut Failures) -> Option<u8> {
    let members = object(Some(value), "/tiles", failures)?;
    unknown(
        members,
        &["max_level"],
        "/tiles",
        "the tiles wanted",
        failures,
    );
    member::<u8>(members, "/tiles", "max_level", false, failures)
}
