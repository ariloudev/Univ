//! An object's `entries` rows: each catalogue entry with its designation, how it was resolved,
//! and its sky position as published.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::columns::Columns;
use super::measurements::from_extra;
use super::{ReadError, Row, Vocabulary};
use crate::api::objects::{EntryPosition, EntryRecord, SpectralTypeRecord};
use crate::entry::SpectralType;
use crate::entry::{PositionErrors, PositionUncertainty};
use crate::ids::{Designation, SourceId};
use crate::object::{Confidence, LineOfSight, MatchRecord, Resolution};
use crate::units::Unit;
use crate::value::{KindOfValue, Quantity, Uncertainty, Value as Measured};

const TABLE: &str = "entries";

/// The designation of source `source` and `identifier`.
fn designation(c: &Columns<'_>, source: &str, identifier: &str) -> Result<Designation, ReadError> {
    let source =
        SourceId::new(source).map_err(|e| c.error("source", format!("{source:?}: {e}")))?;
    Designation::new(source, identifier.to_owned())
        .map_err(|e| c.error("identifier", format!("{identifier:?}: {e}")))
}

/// The designation a row of `c` gives: its source by number (`source`) and its `identifier`.
fn coded(v: &Vocabulary, c: &Columns<'_>) -> Result<Designation, ReadError> {
    let n = c.int("source")?;
    let source = v
        .source(n)
        .ok_or_else(|| c.error("source", format!("{n} is not among the release's sources")))?;
    designation(c, source, c.text("identifier")?)
}

/// The entry's number, source id and identifier, which other entries' resolutions name.
pub(super) fn head(v: &Vocabulary, row: &Row) -> Result<(u32, Designation), ReadError> {
    let c = Columns::new(v, TABLE, row);
    Ok((c.small("entry")?, coded(v, &c)?))
}

/// The designations of an object's `designations` rows, each its source by number (`source`)
/// and `identifier`, as the table holds them.
pub(super) fn designations(v: &Vocabulary, rows: &[Row]) -> Result<Vec<Designation>, ReadError> {
    rows.iter()
        .map(|row| coded(v, &Columns::new(v, "designations", row)))
        .collect()
}

/// Every entry of the object: `rows` by number, sorted, `heads` their designations by number.
pub(super) fn entries(
    v: &Vocabulary,
    rows: &[(u32, &Row)],
    heads: &BTreeMap<u32, Designation>,
) -> Result<Vec<EntryRecord>, ReadError> {
    rows.iter()
        .map(|(number, row)| entry(v, row, *number, heads))
        .collect()
}

/// Entry `number`, its row `row`, its own designation among `heads`.
fn entry(
    v: &Vocabulary,
    row: &Row,
    number: u32,
    heads: &BTreeMap<u32, Designation>,
) -> Result<EntryRecord, ReadError> {
    let c = Columns::new(v, TABLE, row);
    let own = heads
        .get(&number)
        .ok_or_else(|| c.error("entry", format!("{number} has no designation")))?;
    let extra: Map<String, Value> = c.json_opt("extra")?.unwrap_or_default();
    let flags: BTreeMap<String, String> = c.json_opt("flags")?.unwrap_or_default();
    Ok(EntryRecord {
        entry: number,
        source: own.source.as_str().to_owned(),
        identifier: own.identifier.clone(),
        name: c.text_opt("name")?.map(str::to_owned),
        resolved_by: resolution(&c, own, &extra, heads)?,
        position: position(&c, &extra)?,
        group_id: c.text_opt("group_id")?.map(str::to_owned),
        spectral_type: spectral_type(&c, &extra)?,
        references: from_extra(&c, &extra, "references")?.unwrap_or_default(),
        flags,
    })
}

/// The entry it was resolved through: the object's entry `via_entry`, or the designation
/// `extra.via` when that is none of the object's entries.
fn via(
    c: &Columns<'_>,
    extra: &Map<String, Value>,
    heads: &BTreeMap<u32, Designation>,
) -> Result<Designation, ReadError> {
    if let Some(n) = c.small_opt("via_entry")? {
        return heads
            .get(&n)
            .cloned()
            .ok_or_else(|| c.error("via_entry", format!("{n} is none of the object's entries")));
    }
    let via = extra
        .get("via")
        .ok_or_else(|| c.error("via_entry", "is NULL and extra names no via"))?;
    Designation::deserialize(via).map_err(|e| c.error("extra", format!("via {via}: {e}")))
}

fn confidence(c: &Columns<'_>) -> Result<Confidence, ReadError> {
    let x = c.real("confidence")?;
    Confidence::new(x).map_err(|e| c.error("confidence", e))
}

fn resolution(
    c: &Columns<'_>,
    own: &Designation,
    extra: &Map<String, Value>,
    heads: &BTreeMap<u32, Designation>,
) -> Result<Resolution, ReadError> {
    Ok(match c.term("resolution")? {
        "anchor" => Resolution::Anchor,
        "cross_identification" => Resolution::CrossIdentification {
            scheme: c.term("scheme")?.to_owned(),
            identifier: c
                .text_opt("shared")?
                .map_or_else(|| own.identifier.clone(), str::to_owned),
            via: via(c, extra, heads)?,
            confidence: confidence(c)?,
        },
        "match" => {
            let delta = c.real_opt("delta")?;
            let difference = match (c.term_opt("difference")?, delta) {
                (None, _) => None,
                (Some("velocity"), Some(delta_km_s)) => Some(LineOfSight::Velocity { delta_km_s }),
                (Some("distance"), Some(delta_fraction)) => {
                    Some(LineOfSight::Distance { delta_fraction })
                }
                (Some(other), _) => {
                    return Err(c.error("difference", format!("{other:?} with delta {delta:?}")));
                }
            };
            Resolution::Match(MatchRecord {
                method: c.typed("scheme")?,
                with: via(c, extra, heads)?,
                separation_arcsec: c.real("separation_arcsec")?,
                difference,
                confidence: confidence(c)?,
            })
        }
        other => return Err(c.error("resolution", format!("{other:?} is no resolution"))),
    })
}

/// The spectral type `extra` holds, where its source publishes one.
fn spectral_type(
    c: &Columns<'_>,
    extra: &Map<String, Value>,
) -> Result<Option<SpectralTypeRecord>, ReadError> {
    Ok(
        from_extra::<SpectralType>(c, extra, "spectral_type")?.map(|t| SpectralTypeRecord {
            text: t.text,
            kind_of_value: t.kind,
        }),
    )
}

/// The entry's sky position as published, none when it has none.
fn position(
    c: &Columns<'_>,
    extra: &Map<String, Value>,
) -> Result<Option<EntryPosition>, ReadError> {
    let Some(frame) = c.typed_opt("frame")? else {
        return Ok(None);
    };
    let (lon, lat) = (c.real("lon")?, c.real("lat")?);
    let unit: Unit = c.typed("position_unit")?;
    // NULL where the native position is ICRS degrees already.
    let (ra, dec) = match (c.real_opt("ra")?, c.real_opt("dec")?) {
        (Some(ra), Some(dec)) => (ra, dec),
        (None, None) => (lon, lat),
        _ => return Err(c.error("ra", "ra and dec are not both NULL or both set")),
    };
    let uncertainty = if let Some(errors) = extra.get("position_uncertainty") {
        PositionUncertainty::deserialize(errors)
            .map_err(|e| c.error("extra", format!("position_uncertainty {errors}: {e}")))?
    } else if let Some(error_ra) = c.real_opt("error_ra")? {
        let unit: Unit = c.typed("error_unit")?;
        let kind: KindOfValue = c.typed("error_kind")?;
        let error = |value: f64, column: &str| -> Result<Measured, ReadError> {
            let native = Quantity::new(value, unit, Uncertainty::NoneReported)
                .map_err(|e| c.error(column, e))?;
            let normalized = native
                .to(Unit::Degree)
                .map_err(|e| c.error("error_unit", e))?;
            Ok(Measured {
                native,
                normalized,
                kind,
            })
        };
        PositionUncertainty::Published(Box::new(PositionErrors {
            ra: error(error_ra, "error_ra")?,
            dec: error(c.real("error_dec")?, "error_dec")?,
        }))
    } else {
        PositionUncertainty::NoneReported
    };
    Ok(Some(EntryPosition {
        frame,
        lon,
        lat,
        unit,
        epoch: c.real("epoch")?,
        ra,
        dec,
        uncertainty,
    }))
}
