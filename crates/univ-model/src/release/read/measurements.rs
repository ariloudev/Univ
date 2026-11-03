//! An object's `measurements` rows: each value as published beside its normalized one, a
//! distance with its kind, method and measure, a redshift with its CMB-frame value.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::columns::Columns;
use super::{ReadError, Row, Vocabulary};
use crate::api::objects::{DistanceDetail, MeasuredQuantity, MeasurementRecord, RedshiftDetail};
use crate::ids::Designation;
use crate::redshift::RedshiftFrame;
use crate::units::Unit;

const TABLE: &str = "measurements";

/// A measurement's key: (entry, quantity, n).
pub(super) type Key = (u32, MeasuredQuantity, u32);

/// The measurement's key.
pub(super) fn key(v: &Vocabulary, row: &Row) -> Result<Key, ReadError> {
    let c = Columns::new(v, TABLE, row);
    Ok((c.small("entry")?, c.typed("quantity")?, c.small("n")?))
}

/// What of `extra` a column of type `T` holds, none when it is absent.
fn from_extra<T: serde::de::DeserializeOwned>(
    c: &Columns<'_>,
    extra: &Map<String, Value>,
    field: &str,
) -> Result<Option<T>, ReadError> {
    extra
        .get(field)
        .map(|x| T::deserialize(x).map_err(|e| c.error("extra", format!("{field} {x}: {e}"))))
        .transpose()
}

/// The measurement of `row`, its key `key` ([`key`]), `heads` the object's entries by number.
pub(super) fn measurement(
    v: &Vocabulary,
    row: &Row,
    (entry, quantity, n): Key,
    heads: &BTreeMap<u32, Designation>,
) -> Result<MeasurementRecord, ReadError> {
    let c = Columns::new(v, TABLE, row);
    let source = heads
        .get(&entry)
        .ok_or_else(|| c.error("entry", format!("{entry} is none of the object's entries")))?
        .source
        .as_str()
        .to_owned();
    let native = c.quantity(c.real("native_value")?, c.typed("native_unit")?, "native_")?;
    let normalized = c
        .real_opt("value")?
        .map(|x| c.quantity(x, c.typed("unit")?, ""))
        .transpose()?;
    let extra: Map<String, Value> = c.json_opt("extra")?.unwrap_or_default();
    let mut record = MeasurementRecord {
        entry,
        source,
        quantity,
        n,
        kind_of_value: c.typed("kind_of_value")?,
        native,
        normalized,
        distance: None,
        redshift: None,
        band: None,
    };
    match quantity {
        MeasuredQuantity::Distance => {
            record.distance = Some(DistanceDetail {
                kind: c.typed("kind")?,
                method: c.typed("method")?,
                measure: c.typed("measure")?,
                also_published: from_extra(&c, &extra, "also_published")?,
                assumed_cosmology: from_extra(&c, &extra, "assumed_cosmology")?,
            });
        }
        MeasuredQuantity::Redshift => {
            let published_frame =
                from_extra(&c, &extra, "published_frame")?.unwrap_or(RedshiftFrame::Heliocentric);
            record.redshift = Some(RedshiftDetail {
                kind: c.typed("kind")?,
                published_frame,
                dipole: c.typed("method")?,
                cmb: c.quantity(c.real("cmb")?, Unit::Dimensionless, "cmb_")?,
            });
        }
        MeasuredQuantity::Magnitude => record.band = Some(c.term("kind")?.to_owned()),
        _ => {}
    }
    Ok(record)
}
