//! An object's catalogue entries and their measurements as D1 rows, each value in typed
//! columns, its native value beside its normalized one; an entry's flags and papers, and a
//! measurement's details, in rows of their own (see the schema).

use univ_model::entry::{CatalogueEntry, Position, PositionUncertainty};
use univ_model::ids::Designation;
use univ_model::object::ObjectEntry;
use univ_model::position::Frame;
use univ_model::redshift::RedshiftFrame;
use univ_model::release::UncertaintyColumns;
use univ_model::release::rows::{EntryRow, MeasurementRow};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

use super::d1::D1Writer;
use super::extras::{detail_row, flag_rows, reason, reference_rows};
use super::kept::Kept;
use super::terms::{Chains, SourceIndex, Terms};

/// What numbers the rows of a release: its number, terms, sources and transformation chains.
#[derive(Clone)]
pub struct Numbering {
    pub release: u32,
    pub terms: Terms,
    pub sources: SourceIndex,
    pub chains: Chains,
}

impl Numbering {
    /// The numbering of release `release`, its sources indexed by `sources`, keeping the terms
    /// and chains `previous`, the release before, numbered.
    #[must_use]
    pub fn kept(release: u32, sources: SourceIndex, previous: &Kept) -> Self {
        Self {
            release,
            terms: Terms::kept(&previous.terms),
            sources,
            chains: Chains::kept(&previous.chains),
        }
    }
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

/// The entry row of the `n`-th of `entries`, of object `object_id`, and the rows of its flags
/// and papers, written through `w`; the errors of its position that its row cannot hold come
/// back to be written as its measurements.
///
/// # Errors
///
/// Fails when a value cannot be written as a term or a row cannot be written.
pub fn write_entry<'e>(
    writer: &mut D1Writer<'_>,
    numbering: &mut Numbering,
    object_id: u64,
    entries: &'e [ObjectEntry],
    n: usize,
) -> Result<(EntryRow, Option<&'e PositionUncertainty>), String> {
    let ObjectEntry { entry, resolved_by } = &entries[n];
    let own = &entry.designation;
    let r = reason(numbering, own, resolved_by)?;
    let via_entry = r.via.and_then(|through| entry_number(entries, through));
    let via = r.via.filter(|_| via_entry.is_none());
    let terms = &mut numbering.terms;
    let mut row = EntryRow {
        release: numbering.release,
        object_id,
        entry: small(n),
        source: numbering.sources.n(&own.source),
        identifier: String::from(&*own.identifier),
        resolution: r.resolution,
        scheme: r.scheme,
        shared: r.shared,
        via_entry,
        confidence: r.confidence,
        separation_arcsec: r.separation_arcsec,
        difference: r.difference,
        delta: r.delta,
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
        via_source: None,
        via_identifier: via.map(|through| String::from(&*through.identifier)),
        spectral_type: entry.spectral_type.as_ref().map(|t| t.text.clone()),
        spectral_type_kind: entry
            .spectral_type
            .as_ref()
            .map(|t| terms.of(&t.kind))
            .transpose()?,
    };
    row.via_source = via.map(|through| numbering.sources.n(&through.source));
    let published = match &entry.position {
        Some(p) => position_columns(&mut numbering.terms, &mut row, p)?,
        None => None,
    };
    writer.add(&row)?;
    let release = numbering.release;
    for flag in flag_rows(numbering, object_id, row.entry, entry.flags.iter()) {
        writer.add(&flag)?;
    }
    for paper in reference_rows(release, object_id, row.entry, &entry.references) {
        writer.add(&paper)?;
    }
    Ok((row, published))
}

/// The columns of an entry's sky position; its errors when no column holds them, to be written
/// as its measurements.
fn position_columns<'p>(
    terms: &mut Terms,
    row: &mut EntryRow,
    p: &'p Position,
) -> Result<Option<&'p PositionUncertainty>, String> {
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
                return Ok(Some(&p.uncertainty));
            }
        }
    }
    Ok(None)
}

/// What a measurement row holds beside its values, and what its `measurement_details` row
/// holds.
#[derive(Clone, Copy, Default)]
struct Detail<'q> {
    kind: Option<u32>,
    method: Option<u32>,
    measure: Option<u32>,
    cmb: Option<&'q Quantity>,
    published_frame: Option<u32>,
    also_published: Option<&'q Quantity>,
    assumed_cosmology: Option<&'q univ_model::cosmology::AssumedCosmology>,
    derived_from: Option<&'q [String; 2]>,
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
            published_frame: detail.published_frame,
        };
        self.writer.add(&row)?;
        let details = detail_row(
            numbering,
            self.object_id,
            self.entry,
            row.quantity,
            row.n,
            detail.also_published,
            detail.assumed_cosmology,
            detail.derived_from,
        )?;
        match details {
            Some(details) => self.writer.add(&details),
            None => Ok(()),
        }
    }

    /// The published errors of the entry's position that its row cannot hold, as the
    /// measurements `position_error_ra` and `position_error_dec`.
    ///
    /// # Errors
    ///
    /// Fails when a value cannot be written as a term or a row cannot be written.
    pub fn position_errors(
        &mut self,
        numbering: &mut Numbering,
        uncertainty: &PositionUncertainty,
    ) -> Result<(), String> {
        if let PositionUncertainty::Published(errors) = uncertainty {
            self.value(numbering, "position_error_ra", &errors.ra, None)?;
            self.value(numbering, "position_error_dec", &errors.dec, None)?;
        }
        Ok(())
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
            let terms = &mut numbering.terms;
            let detail = Detail {
                kind: Some(terms.of(&d.kind)?),
                method: Some(terms.of(&d.method)?),
                measure: Some(terms.of(&d.measure)?),
                also_published: d.also_published.as_ref(),
                assumed_cosmology: d.assumed_cosmology.as_ref(),
                ..Detail::default()
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
            let native = z.velocity.as_deref().unwrap_or(&published);
            let terms = &mut numbering.terms;
            let published_frame = match z.cmb.published {
                RedshiftFrame::Heliocentric => None,
                frame => Some(terms.of(&frame)?),
            };
            let detail = Detail {
                kind: Some(terms.of(&z.kind)?),
                method: Some(terms.of(&z.cmb.dipole)?),
                cmb: Some(&z.cmb.value),
                published_frame,
                ..Detail::default()
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
        self.write_colours_and_ages(numbering, entry)
    }

    /// The colour indices and ages of `entry`, each numbered within the entry, as its distances
    /// are.
    fn write_colours_and_ages(
        &mut self,
        numbering: &mut Numbering,
        entry: &CatalogueEntry,
    ) -> Result<(), String> {
        for (n, c) in entry.colours.iter().enumerate() {
            // Numbered within the entry, as its distances are; the bands are its kind.
            let detail = Detail {
                kind: Some(numbering.terms.id(&c.band_pair())),
                derived_from: c.derived_from.as_ref(),
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
        for (n, a) in entry.ages.iter().enumerate() {
            let v = &a.value;
            let detail = Detail::default();
            self.push(
                numbering,
                "age",
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
