//! What an object's entries and measurements hold beside their own rows (schema version 8): each
//! entry's flags (`entry_flags`), the papers its values come from (`entry_references`) and its
//! published position errors where the error columns cannot hold them (measurements of
//! `position_error_ra` and `position_error_dec`), and each measurement's details
//! (`measurement_details`).

use std::collections::BTreeMap;

use super::columns::Columns;
use super::measurements::{Key, key};
use super::{ReadError, Row, Vocabulary};
use crate::entry::{PaperReference, PositionErrors};
use crate::value::{Quantity, Value as Measured};

/// The quantities of a published position's errors, in right ascension and declination, which
/// are measurements of no [`crate::api::objects::MeasuredQuantity`]: they belong to the entry's
/// position. Univ Ingest writes them under these names.
pub const POSITION_ERRORS: [&str; 2] = ["position_error_ra", "position_error_dec"];

/// What each of an object's entries holds beside its row, by entry number.
#[derive(Default)]
pub(super) struct EntryExtras {
    pub flags: BTreeMap<u32, BTreeMap<String, String>>,
    pub references: BTreeMap<u32, Vec<PaperReference>>,
    pub position_errors: BTreeMap<u32, PositionErrors>,
}

/// Each entry's flags, by column name, from the object's `entry_flags` rows.
pub(super) fn flags(
    v: &Vocabulary,
    rows: &[Row],
) -> Result<BTreeMap<u32, BTreeMap<String, String>>, ReadError> {
    let mut flags: BTreeMap<u32, BTreeMap<String, String>> = BTreeMap::new();
    for row in rows {
        let c = Columns::new(v, "entry_flags", row);
        flags
            .entry(c.small("entry")?)
            .or_default()
            .insert(c.term("flag")?.to_owned(), c.text("value")?.to_owned());
    }
    Ok(flags)
}

/// Each entry's papers, in their order, from the object's `entry_references` rows.
pub(super) fn references(
    v: &Vocabulary,
    rows: &[Row],
) -> Result<BTreeMap<u32, Vec<PaperReference>>, ReadError> {
    let mut numbered: BTreeMap<u32, BTreeMap<u32, PaperReference>> = BTreeMap::new();
    for row in rows {
        let c = Columns::new(v, "entry_references", row);
        let paper = PaperReference {
            citation: c.text("citation")?.to_owned(),
            doi: c.text_opt("doi")?.map(str::to_owned),
            bibcode: c.text_opt("bibcode")?.map(str::to_owned),
        };
        numbered
            .entry(c.small("entry")?)
            .or_default()
            .insert(c.small("n")?, paper);
    }
    Ok(numbered
        .into_iter()
        .map(|(entry, papers)| (entry, papers.into_values().collect()))
        .collect())
}

/// The value of a `measurements` row: as published, and normalized where that differs.
fn value(c: &Columns<'_>) -> Result<Measured, ReadError> {
    let native = c.quantity(c.real("native_value")?, c.typed("native_unit")?, "native_")?;
    let normalized: Quantity = match c.real_opt("value")? {
        Some(x) => c.quantity(x, c.typed("unit")?, "")?,
        None => native,
    };
    Ok(Measured {
        native,
        normalized,
        kind: c.typed("kind_of_value")?,
    })
}

/// The object's `measurements` rows split into each entry's published position errors and
/// every other measurement.
pub(super) fn position_errors<'r>(
    v: &Vocabulary,
    rows: &'r [Row],
) -> Result<(BTreeMap<u32, PositionErrors>, Vec<&'r Row>), ReadError> {
    let mut halves: BTreeMap<u32, [Option<Measured>; 2]> = BTreeMap::new();
    let mut others = Vec::with_capacity(rows.len());
    for row in rows {
        let c = Columns::new(v, "measurements", row);
        let quantity = c.term("quantity")?;
        match POSITION_ERRORS.iter().position(|q| *q == quantity) {
            Some(axis) => halves.entry(c.small("entry")?).or_default()[axis] = Some(value(&c)?),
            None => others.push(row),
        }
    }
    let errors = halves
        .into_iter()
        .map(|(entry, [ra, dec])| match (ra, dec) {
            (Some(ra), Some(dec)) => Ok((entry, PositionErrors { ra, dec })),
            _ => Err(ReadError(format!(
                "entry {entry} has one of its position errors and not the other"
            ))),
        })
        .collect::<Result<_, _>>()?;
    Ok((errors, others))
}

/// The object's `measurement_details` rows by the key of the measurement each belongs to.
pub(super) fn by_key<'r>(
    v: &Vocabulary,
    rows: &'r [Row],
) -> Result<BTreeMap<Key, &'r Row>, ReadError> {
    rows.iter()
        .map(|row| Ok((key(v, "measurement_details", row)?, row)))
        .collect()
}
