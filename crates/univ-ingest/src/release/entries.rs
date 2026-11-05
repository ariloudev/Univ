//! An object's catalogue entries and their measurements as D1 rows, each value in typed
//! columns, its native value beside its normalized one, and what no column holds kept as JSON
//! in `extra` (see the schema).

use serde_json::{Map, Value as Json};
use univ_model::entry::{CatalogueEntry, Position, PositionUncertainty};
use univ_model::ids::Designation;
use univ_model::object::{LineOfSight, ObjectEntry, Resolution};
use univ_model::position::Frame;
use univ_model::redshift::RedshiftFrame;
use univ_model::release::UncertaintyColumns;
use univ_model::release::rows::{EntryRow, MeasurementRow};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

use super::d1::D1Writer;
use super::json;
use super::terms::{SourceIndex, Terms};

/// What numbers the rows of a release: its number, terms and sources.
#[derive(Clone)]
pub struct Numbering {
    pub release: u32,
    pub terms: Terms,
    pub sources: SourceIndex,
}

/// `n` as a column's integer.
pub fn small(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The number of the entry of `entries` designated `d`.
pub fn entry_number(entries: &[ObjectEntry], d: &Designation) -> Option<u32> {
    entries
        .iter()
        .position(|e| &e.entry.designation == d)
        .map(small)
}

/// `extra` as a column: none when it holds nothing.
fn extra(extra: &Map<String, Json>) -> Result<Option<String>, String> {
    if extra.is_empty() {
        Ok(None)
    } else {
        json(extra).map(Some)
    }
}

/// `value` as JSON, for `extra`.
fn to_json<T: serde::Serialize>(value: &T) -> Result<Json, String> {
    serde_json::to_value(value).map_err(|e| format!("a value cannot be written as JSON: {e}"))
}

/// The entry row of the `n`-th of `entries`, of object `object_id`.
///
/// # Errors
///
/// Fails when a value cannot be written as JSON.
pub fn entry_row(
    numbering: &mut Numbering,
    object_id: u64,
    entries: &[ObjectEntry],
    n: usize,
) -> Result<EntryRow, String> {
    let ObjectEntry { entry, resolved_by } = &entries[n];
    let terms = &mut numbering.terms;
    let d = &entry.designation;
    let mut more = Map::new();
    let via = |through: &Designation, more: &mut Map<String, Json>| {
        let found = entry_number(entries, through);
        if found.is_none() {
            more.insert("via".into(), to_json(through)?);
        }
        Ok::<_, String>(found)
    };
    let mut row = EntryRow {
        release: numbering.release,
        object_id,
        entry: small(n),
        source: numbering.sources.n(&d.source),
        identifier: d.identifier.to_string(),
        resolution: terms.id("anchor"),
        scheme: None,
        shared: None,
        via_entry: None,
        confidence: None,
        separation_arcsec: None,
        difference: None,
        delta: None,
        name: entry.name.as_deref().map(str::to_owned),
        frame: None,
        lon: None,
        lat: None,
        position_unit: None,
        epoch: None,
        ra: None,
        dec: None,
        error_ra: None,
        error_dec: None,
        error_unit: None,
        error_kind: None,
        group_id: entry.group_id.as_deref().map(str::to_owned),
        flags: (!entry.flags.is_empty())
            .then(|| json(&entry.flags))
            .transpose()?,
        extra: None,
    };
    match &**resolved_by {
        Resolution::Anchor => {}
        Resolution::CrossIdentification {
            scheme,
            identifier,
            via: through,
            confidence,
        } => {
            row.resolution = terms.id("cross_identification");
            row.scheme = Some(terms.id(scheme));
            row.shared = (**identifier != *d.identifier).then(|| identifier.clone());
            row.via_entry = via(through, &mut more)?;
            row.confidence = Some(confidence.get());
        }
        Resolution::Match(m) => {
            row.resolution = terms.id("match");
            row.scheme = Some(terms.of(&m.method)?);
            row.via_entry = via(&m.with, &mut more)?;
            row.separation_arcsec = Some(m.separation_arcsec);
            (row.difference, row.delta) = match m.difference {
                None => (None, None),
                Some(LineOfSight::Velocity { delta_km_s }) => {
                    (Some(terms.id("velocity")), Some(delta_km_s))
                }
                Some(LineOfSight::Distance { delta_fraction }) => {
                    (Some(terms.id("distance")), Some(delta_fraction))
                }
            };
            row.confidence = Some(m.confidence.get());
        }
    }
    if let Some(p) = &entry.position {
        position_columns(terms, &mut row, &mut more, p)?;
    }
    if let Some(t) = &entry.spectral_type {
        more.insert("spectral_type".into(), to_json(t)?);
    }
    if !entry.references.is_empty() {
        more.insert("references".into(), to_json(&entry.references)?);
    }
    row.extra = extra(&more)?;
    Ok(row)
}

/// The columns of an entry's sky position, and its errors in `more` when no column holds them.
fn position_columns(
    terms: &mut Terms,
    row: &mut EntryRow,
    more: &mut Map<String, Json>,
    p: &Position,
) -> Result<(), String> {
    let native = &p.native;
    row.frame = Some(terms.of(&native.frame)?);
    row.lon = Some(native.lon);
    row.lat = Some(native.lat);
    row.position_unit = Some(terms.of(&native.unit)?);
    row.epoch = Some(native.epoch.0);
    let as_published = native.frame == Frame::Icrs
        && native.unit == Unit::Degree
        && p.icrs.ra.total_cmp(&native.lon).is_eq()
        && p.icrs.dec.total_cmp(&native.lat).is_eq()
        && p.icrs.epoch == native.epoch;
    if !as_published {
        row.ra = Some(p.icrs.ra);
        row.dec = Some(p.icrs.dec);
    }
    match &p.uncertainty {
        PositionUncertainty::NoneReported => {}
        PositionUncertainty::Published(errors) => {
            let typed = |v: &Value| {
                v.native.uncertainty == Uncertainty::NoneReported
                    && v.native.to(Unit::Degree).is_ok_and(|d| d == v.normalized)
            };
            let (ra, dec) = (&errors.ra, &errors.dec);
            if typed(ra) && typed(dec) && ra.native.unit == dec.native.unit && ra.kind == dec.kind {
                row.error_ra = Some(ra.native.value);
                row.error_dec = Some(dec.native.value);
                row.error_unit = Some(terms.of(&ra.native.unit)?);
                row.error_kind = Some(terms.of(&ra.kind)?);
            } else {
                more.insert("position_uncertainty".into(), to_json(&p.uncertainty)?);
            }
        }
    }
    Ok(())
}

/// What a measurement row holds beside its values.
#[derive(Default)]
struct Detail<'q> {
    kind: Option<u32>,
    method: Option<u32>,
    measure: Option<u32>,
    cmb: Option<&'q Quantity>,
    extra: Option<String>,
}

/// The measurements of one entry, written as rows.
pub struct Measurements<'w, 's> {
    pub writer: &'w mut D1Writer<'s>,
    pub object_id: u64,
    pub entry: u32,
}

impl Measurements<'_, '_> {
    #[expect(
        clippy::too_many_arguments,
        reason = "a row's own columns, each named where it is written"
    )]
    fn push(
        &mut self,
        numbering: &mut Numbering,
        quantity: &str,
        n: usize,
        kind_of_value: KindOfValue,
        native: &Quantity,
        normalized: &Quantity,
        detail: Detail<'_>,
    ) -> Result<(), String> {
        let terms = &mut numbering.terms;
        let u = UncertaintyColumns::from(native.uncertainty);
        let same = normalized == native;
        let v = UncertaintyColumns::from(normalized.uncertainty);
        let cmb = detail
            .cmb
            .map(|q| (q.value, UncertaintyColumns::from(q.uncertainty)));
        let row = MeasurementRow {
            release: numbering.release,
            object_id: self.object_id,
            entry: self.entry,
            quantity: terms.id(quantity),
            n: small(n),
            kind_of_value: terms.of(&kind_of_value)?,
            native_value: native.value,
            native_unit: terms.of(&native.unit)?,
            native_form: terms.id(u.form),
            native_minus: u.minus,
            native_plus: u.plus,
            native_level: u.level,
            value: (!same).then_some(normalized.value),
            unit: (!same).then(|| terms.of(&normalized.unit)).transpose()?,
            form: (!same).then(|| terms.id(v.form)),
            minus: v.minus.filter(|_| !same),
            plus: v.plus.filter(|_| !same),
            level: v.level.filter(|_| !same),
            kind: detail.kind,
            method: detail.method,
            measure: detail.measure,
            cmb: cmb.map(|(z, _)| z),
            cmb_form: cmb.map(|(_, c)| terms.id(c.form)),
            cmb_minus: cmb.and_then(|(_, c)| c.minus),
            cmb_plus: cmb.and_then(|(_, c)| c.plus),
            cmb_level: cmb.and_then(|(_, c)| c.level),
            extra: detail.extra,
        };
        self.writer.add(&row)
    }

    /// A plain value: its native and normalized quantities and kind.
    fn value(
        &mut self,
        numbering: &mut Numbering,
        quantity: &str,
        v: &Value,
        kind: Option<u32>,
    ) -> Result<(), String> {
        let detail = Detail {
            kind,
            ..Detail::default()
        };
        self.push(
            numbering,
            quantity,
            0,
            v.kind,
            &v.native,
            &v.normalized,
            detail,
        )
    }

    /// Every measurement of `entry`.
    ///
    /// # Errors
    ///
    /// Fails when a value cannot be written as JSON or a row cannot be written.
    pub fn write(
        &mut self,
        numbering: &mut Numbering,
        entry: &CatalogueEntry,
    ) -> Result<(), String> {
        for (n, d) in entry.distances.iter().enumerate() {
            let mut more = Map::new();
            if let Some(also) = &d.also_published {
                more.insert("also_published".into(), to_json(also)?);
            }
            if let Some(assumed) = &d.assumed_cosmology {
                more.insert("assumed_cosmology".into(), to_json(assumed)?);
            }
            let terms = &mut numbering.terms;
            let detail = Detail {
                kind: Some(terms.of(&d.kind)?),
                method: Some(terms.of(&d.method)?),
                measure: Some(terms.of(&d.measure)?),
                cmb: None,
                extra: extra(&more)?,
            };
            self.push(
                numbering,
                "distance",
                n,
                d.kind_of_value,
                &d.native,
                &d.normalized,
                detail,
            )?;
        }
        for (n, z) in entry.redshifts.iter().enumerate() {
            // As published: the velocity cz where the source gives one, else z; normalized to
            // the z of the frame it was published in, beside its CMB-frame value. A redshift
            // published in a frame other than heliocentric names it: in the CMB frame no dipole
            // moved it there, and a barycentric one is never relabelled heliocentric.
            let published = z.cmb.as_published();
            let native = z.velocity.as_ref().unwrap_or(&published);
            let frame = match z.cmb.published {
                RedshiftFrame::Heliocentric => None,
                RedshiftFrame::Cmb => Some(r#"{"published_frame":"cmb"}"#),
                RedshiftFrame::Barycentric => Some(r#"{"published_frame":"barycentric"}"#),
                RedshiftFrame::LocalGroup => Some(r#"{"published_frame":"local_group"}"#),
            };
            let terms = &mut numbering.terms;
            let detail = Detail {
                kind: Some(terms.of(&z.kind)?),
                method: Some(terms.of(&z.cmb.dipole)?),
                measure: None,
                cmb: Some(&z.cmb.value),
                extra: frame.map(str::to_owned),
            };
            self.push(
                numbering,
                "redshift",
                n,
                z.kind_of_value,
                native,
                &published,
                detail,
            )?;
        }
        let pm = entry.proper_motion.as_ref();
        let sgv = entry.supergalactic_velocity.as_ref();
        let values: [(&str, Option<&Value>); 7] = [
            ("parallax", entry.parallax.as_deref()),
            ("velocity", entry.velocity.as_deref()),
            ("proper_motion_ra", pm.map(|p| &p.ra)),
            ("proper_motion_dec", pm.map(|p| &p.dec)),
            ("supergalactic_velocity_x", sgv.map(|v| &v.x)),
            ("supergalactic_velocity_y", sgv.map(|v| &v.y)),
            ("supergalactic_velocity_z", sgv.map(|v| &v.z)),
        ];
        for (quantity, value) in values {
            if let Some(v) = value {
                self.value(numbering, quantity, v, None)?;
            }
        }
        if let Some(m) = &entry.magnitude {
            let band = numbering.terms.id(&m.band);
            self.value(numbering, "magnitude", &m.value, Some(band))?;
        }
        for (n, c) in entry.colours.iter().enumerate() {
            // Numbered within the entry, as its distances are; the bands are its kind.
            let mut more = Map::new();
            if let Some(from) = &c.derived_from {
                more.insert("derived_from".into(), to_json(from)?);
            }
            let detail = Detail {
                kind: Some(numbering.terms.id(&c.band_pair())),
                extra: extra(&more)?,
                ..Detail::default()
            };
            let v = &c.value;
            self.push(
                numbering,
                "colour_index",
                n,
                v.kind,
                &v.native,
                &v.normalized,
                detail,
            )?;
        }
        Ok(())
    }
}
