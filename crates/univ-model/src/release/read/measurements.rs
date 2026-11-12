//! An object's `measurements` rows: each value as published beside its normalized one, a
//! distance with its kind, method and measure, a redshift with its CMB-frame value, and the few
//! details a `measurement_details` row adds.

use std::collections::BTreeMap;

use super::columns::Columns;
use super::{ReadError, Row, Vocabulary};
use crate::api::objects::{
    DistanceDetail, MeasuredQuantity, MeasurementRecord, RedshiftDetail, UncertaintyBasis,
};
use crate::cosmology::AssumedCosmology;
use crate::ids::Designation;
use crate::redshift::RedshiftFrame;
use crate::units::Unit;
use crate::value::KindOfValue;

const TABLE: &str = "measurements";

/// A measurement's key: (entry, quantity, n).
pub(super) type Key = (u32, MeasuredQuantity, u32);

/// The key of a row of `table`, `measurements` or `measurement_details`.
pub(super) fn key(v: &Vocabulary, table: &'static str, row: &Row) -> Result<Key, ReadError> {
    let c = Columns::new(v, table, row);
    Ok((c.small("entry")?, c.typed("quantity")?, c.small("n")?))
}

/// A distance's value as also published, from its details `d`; none where it has none.
fn also_published(d: &Columns<'_>) -> Result<Option<crate::value::Quantity>, ReadError> {
    d.real_opt("also_value")?
        .map(|x| d.quantity(x, d.typed("also_unit")?, "also_"))
        .transpose()
}

/// The cosmology a distance assumes, from its details `d`; none where it states none.
fn assumed_cosmology(d: &Columns<'_>) -> Result<Option<AssumedCosmology>, ReadError> {
    d.real_opt("assumed_hubble_constant")?
        .map(|h| {
            AssumedCosmology::new(
                h,
                d.real_opt("assumed_omega_matter")?,
                d.real_opt("assumed_omega_lambda")?,
            )
            .map_err(|e| d.error("assumed_hubble_constant", e))
        })
        .transpose()
}

/// The two magnitude columns a colour index was derived from, from its details `d`.
fn derived_from(d: &Columns<'_>) -> Result<Option<[String; 2]>, ReadError> {
    match (
        d.text_opt("derived_from_first")?,
        d.text_opt("derived_from_second")?,
    ) {
        (None, None) => Ok(None),
        (Some(a), Some(b)) => Ok(Some([a.to_owned(), b.to_owned()])),
        _ => Err(d.error("derived_from_first", "only one of the two columns is set")),
    }
}

/// The measurement of `row`, its key `key` ([`key`]), `heads` the object's entries by number,
/// with its `measurement_details` row where it has one.
pub(super) fn measurement(
    v: &Vocabulary,
    row: &Row,
    (entry, quantity, n): Key,
    heads: &BTreeMap<u32, Designation>,
    detail: Option<&Row>,
) -> Result<MeasurementRecord, ReadError> {
    let c = Columns::new(v, TABLE, row);
    let d = detail.map(|row| Columns::new(v, "measurement_details", row));
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
    let uncertainty_basis =
        v.catalogue_uncertainty(&source, quantity)
            .map(|basis| UncertaintyBasis {
                kind_of_value: KindOfValue::Estimated,
                basis: basis.to_owned(),
            });
    let mut record = MeasurementRecord {
        entry,
        source,
        quantity,
        n,
        kind_of_value: c.typed("kind_of_value")?,
        uncertainty_basis,
        native,
        normalized,
        distance: None,
        redshift: None,
        band: None,
        derived_from: None,
    };
    match quantity {
        MeasuredQuantity::Distance => {
            record.distance = Some(DistanceDetail {
                kind: c.typed("kind")?,
                method: c.typed("method")?,
                measure: c.typed("measure")?,
                also_published: d.as_ref().map(also_published).transpose()?.flatten(),
                assumed_cosmology: d.as_ref().map(assumed_cosmology).transpose()?.flatten(),
            });
        }
        MeasuredQuantity::Redshift => {
            let published_frame = c
                .typed_opt("published_frame")?
                .unwrap_or(RedshiftFrame::Heliocentric);
            record.redshift = Some(RedshiftDetail {
                kind: c.typed("kind")?,
                published_frame,
                dipole: c.typed("method")?,
                cmb: c.quantity(c.real("cmb")?, Unit::Dimensionless, "cmb_")?,
            });
        }
        MeasuredQuantity::Magnitude => record.band = Some(c.term("kind")?.to_owned()),
        MeasuredQuantity::ColourIndex => {
            record.band = Some(c.term("kind")?.to_owned());
            record.derived_from = d.as_ref().map(derived_from).transpose()?.flatten();
        }
        _ => {}
    }
    Ok(record)
}
