//! Each JSON column of schema 7 in a table that grows with a release's objects, read with
//! univ-model's own types and written as the rows and columns of schema 8 through the builders a
//! build uses, each row valid in the releases its parent is: an entry's flags, papers, the entry
//! it was resolved through, its spectral type and published position errors; a measurement's
//! published frame and details; an object's candidate links and relations; the ids that replaced
//! an id.

use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Map, Value};
use univ_model::cosmology::AssumedCosmology;
use univ_model::entry::{PaperReference, PositionUncertainty, SpectralType};
use univ_model::ids::Designation;
use univ_model::object::{CandidateLink, Relation};
use univ_model::redshift::RedshiftFrame;
use univ_model::release::rows::MeasurementRow;
use univ_model::value::Quantity;

use super::super::entries::position_error_rows;
use super::super::extras::{
    Codes, detail_row, flag_rows, link_rows, reference_rows, relation_rows, replacement_rows,
};
use super::codes::StoreCodes;
use super::sql::{Temp, insert, sql_err};

/// A JSON column's text read as a map, or as `T`, naming the table, row and column.
fn parsed<T: serde::de::DeserializeOwned>(text: &str, at: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| format!("{at}: {text:?} does not read: {e}"))
}

/// The value of `key` in `map` as `T`, none when absent.
fn field<T: serde::de::DeserializeOwned>(
    map: &Map<String, Value>,
    key: &str,
    at: &str,
) -> Result<Option<T>, String> {
    map.get(key)
        .map(|v| T::deserialize(v).map_err(|e| format!("{at}.{key}: {v} does not read: {e}")))
        .transpose()
}

/// Refuses a key of `map` that is not among `known`, which would be lost.
fn only(map: &Map<String, Value>, known: &[&str], at: &str) -> Result<(), String> {
    match map.keys().find(|k| !known.contains(&k.as_str())) {
        Some(k) => Err(format!("{at}: {k:?} is no value schema 8 holds")),
        None => Ok(()),
    }
}

/// A flag's value as the source publishes it: a string as it is, anything else as JSON.
fn flag_text(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// The measurements of published position errors, each with its `retired`, written once
/// `measurements` has schema 8's columns.
pub(super) type Pending = Vec<(MeasurementRow, Option<u32>)>;

/// Writes the rows of `entries.flags` and `entries.extra`, fills `temp` with the columns that
/// replace `extra`'s `via` and `spectral_type`, and gathers in `pending` the measurements of its
/// published position errors. Gives how many entries it read.
pub(super) fn entries(
    db: &Connection,
    codes: &mut StoreCodes,
    temp: &Temp,
    pending: &mut Pending,
) -> Result<u64, String> {
    let sql = "SELECT release, object_id, entry, retired, flags, extra FROM entries \
               WHERE flags IS NOT NULL OR extra IS NOT NULL";
    let mut q = db.prepare(sql).map_err(sql_err(sql))?;
    let mut rows = q.query([]).map_err(sql_err(sql))?;
    let mut read = 0;
    while let Some(r) = rows.next().map_err(sql_err(sql))? {
        let (release, object_id, entry): (u32, u64, u32) = (
            r.get(0).map_err(sql_err(sql))?,
            r.get(1).map_err(sql_err(sql))?,
            r.get(2).map_err(sql_err(sql))?,
        );
        let retired: Option<u32> = r.get(3).map_err(sql_err(sql))?;
        let (flags, extra): (Option<String>, Option<String>) = (
            r.get(4).map_err(sql_err(sql))?,
            r.get(5).map_err(sql_err(sql))?,
        );
        let at = format!("entries ({object_id}, {entry}, release {release})");
        codes.at(release);
        if let Some(text) = flags {
            let map: Map<String, Value> = parsed(&text, &format!("{at}.flags"))?;
            let pairs: Vec<(&str, String)> = map
                .iter()
                .map(|(k, v)| (k.as_str(), flag_text(v)))
                .collect();
            let rows = flag_rows(
                codes,
                object_id,
                entry,
                pairs.iter().map(|(k, v)| (*k, v.as_str())),
            );
            insert(db, &rows, retired)?;
        }
        if let Some(text) = extra {
            let at = format!("{at}.extra");
            let map: Map<String, Value> = parsed(&text, &at)?;
            only(
                &map,
                &["via", "spectral_type", "references", "position_uncertainty"],
                &at,
            )?;
            let via: Option<Designation> = field(&map, "via", &at)?;
            let spectral: Option<SpectralType> = field(&map, "spectral_type", &at)?;
            if via.is_some() || spectral.is_some() {
                let via_source = via.as_ref().map(|d| codes.source(&d.source));
                let kind = spectral
                    .as_ref()
                    .map(|t| codes.term_of(&t.kind))
                    .transpose()?;
                temp.entry(
                    (object_id, entry, release),
                    via_source,
                    via.as_ref().map(|d| &*d.identifier),
                    spectral.as_ref().map(|t| t.text.as_str()),
                    kind,
                )?;
            }
            let papers: Vec<PaperReference> = field(&map, "references", &at)?.unwrap_or_default();
            insert(
                db,
                &reference_rows(release, object_id, entry, &papers),
                retired,
            )?;
            if let Some(u) = field::<PositionUncertainty>(&map, "position_uncertainty", &at)? {
                let rows = position_error_rows(codes, (object_id, entry), &u)?;
                pending.extend(rows.into_iter().map(|r| (r, retired)));
            }
        }
        read += 1;
    }
    Ok(read)
}

/// Writes the rows of `measurements.extra`'s details, and fills `temp` with the published
/// frames that replace its `published_frame`. Gives how many measurements it read.
pub(super) fn measurements(
    db: &Connection,
    codes: &mut StoreCodes,
    temp: &Temp,
) -> Result<u64, String> {
    let sql = "SELECT release, object_id, entry, quantity, n, retired, extra FROM measurements \
               WHERE extra IS NOT NULL";
    let mut q = db.prepare(sql).map_err(sql_err(sql))?;
    let mut rows = q.query([]).map_err(sql_err(sql))?;
    let mut read = 0;
    while let Some(r) = rows.next().map_err(sql_err(sql))? {
        let get = |i| -> Result<u32, String> { r.get(i).map_err(sql_err(sql)) };
        let (release, entry, quantity, n) = (get(0)?, get(2)?, get(3)?, get(4)?);
        let object_id: u64 = r.get(1).map_err(sql_err(sql))?;
        let retired: Option<u32> = r.get(5).map_err(sql_err(sql))?;
        let text: String = r.get(6).map_err(sql_err(sql))?;
        let at = format!(
            "measurements ({object_id}, {entry}, {quantity}, {n}, release {release}).extra"
        );
        let map: Map<String, Value> = parsed(&text, &at)?;
        only(
            &map,
            &[
                "published_frame",
                "also_published",
                "assumed_cosmology",
                "derived_from",
            ],
            &at,
        )?;
        codes.at(release);
        if let Some(frame) = field::<RedshiftFrame>(&map, "published_frame", &at)? {
            let frame = codes.term_of(&frame)?;
            temp.measurement((object_id, entry, quantity, n, release), frame)?;
        }
        let also: Option<Quantity> = field(&map, "also_published", &at)?;
        let assumed: Option<AssumedCosmology> = field(&map, "assumed_cosmology", &at)?;
        let derived: Option<[String; 2]> = field(&map, "derived_from", &at)?;
        let detail = detail_row(
            codes,
            object_id,
            entry,
            quantity,
            n,
            also.as_ref(),
            assumed.as_ref(),
            derived.as_ref(),
        )?;
        insert(db, detail.as_slice(), retired)?;
        read += 1;
    }
    Ok(read)
}

/// A link's reason in the shape releases built before the `by` tag wrote, tagged by `method`,
/// in the shape tagged by `by`. A match's reason then held `method` twice, the tag and its
/// match method, of which a JSON reader keeps the last: a reason that carries a match's
/// `separation_arcsec` is a match, any other the resolution its `method` names.
fn retag(reason: &mut Value) {
    let Value::Object(map) = reason else {
        return;
    };
    if map.contains_key("by") {
        return;
    }
    if map.contains_key("separation_arcsec") {
        map.insert("by".into(), Value::String("match".into()));
    } else if let Some(tag) = map.remove("method") {
        map.insert("by".into(), tag);
    }
}

/// Writes the rows of `objects.links` and `objects.relations`. Gives how many objects it read.
pub(super) fn objects(db: &Connection, codes: &mut StoreCodes) -> Result<u64, String> {
    let sql = "SELECT release, id, retired, links, relations FROM objects \
               WHERE links IS NOT NULL OR relations IS NOT NULL";
    let mut q = db.prepare(sql).map_err(sql_err(sql))?;
    let mut rows = q.query([]).map_err(sql_err(sql))?;
    let mut read = 0;
    while let Some(r) = rows.next().map_err(sql_err(sql))? {
        let release: u32 = r.get(0).map_err(sql_err(sql))?;
        let id: u64 = r.get(1).map_err(sql_err(sql))?;
        let retired: Option<u32> = r.get(2).map_err(sql_err(sql))?;
        let (links, relations): (Option<String>, Option<String>) = (
            r.get(3).map_err(sql_err(sql))?,
            r.get(4).map_err(sql_err(sql))?,
        );
        codes.at(release);
        if let Some(text) = links {
            let at = format!("objects ({id}, release {release}).links");
            let mut value: Value = parsed(&text, &at)?;
            for link in value.as_array_mut().into_iter().flatten() {
                if let Some(reason) = link.get_mut("reason") {
                    retag(reason);
                }
            }
            let links = Vec::<CandidateLink>::deserialize(&value)
                .map_err(|e| format!("{at}: {text:?} does not read: {e}"))?;
            insert(db, &link_rows(codes, id, &links)?, retired)?;
        }
        if let Some(text) = relations {
            let at = format!("objects ({id}, release {release}).relations");
            let relations: Vec<Relation> = parsed(&text, &at)?;
            insert(db, &relation_rows(codes, id, &relations)?, retired)?;
        }
        read += 1;
    }
    Ok(read)
}

/// Writes the rows of `ids_replaced.replaced_by`. Gives how many replacements it read.
pub(super) fn replaced(db: &Connection) -> Result<u64, String> {
    let sql = "SELECT id, release, replaced_by FROM ids_replaced";
    let mut q = db.prepare(sql).map_err(sql_err(sql))?;
    let mut rows = q.query(params![]).map_err(sql_err(sql))?;
    let mut read = 0;
    while let Some(r) = rows.next().map_err(sql_err(sql))? {
        let (id, release): (u64, u32) = (
            r.get(0).map_err(sql_err(sql))?,
            r.get(1).map_err(sql_err(sql))?,
        );
        let text: String = r.get(2).map_err(sql_err(sql))?;
        let by: Vec<u64> = parsed(&text, &format!("ids_replaced ({id}).replaced_by"))?;
        insert(db, &replacement_rows(id, release, &by), None)?;
        read += 1;
    }
    Ok(read)
}
