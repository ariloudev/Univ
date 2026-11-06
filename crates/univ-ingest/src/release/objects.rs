//! A release's objects as D1 rows, written one object at a time: each object with its
//! preferred measurements and derived position, its catalogue entries, every measurement beside
//! its native value, its designations and common names; and the id ledger.
//!
//! An object's derived position is held by its own columns, its preferred measurement, its
//! chain's template and the release (see the schema). The build checks that they give it back
//! for every object (`D1_DERIVED`), so nothing of it is lost.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use univ_model::cosmology::Cosmology;
use univ_model::derive::DerivedPosition;
use univ_model::ids::{Designation, SourceId};
use univ_model::ledger::Ledger;
use univ_model::object::{FractionalUncertainty, MeasurementRef, Object, Placement};
use univ_model::position::Frame;
use univ_model::release::chains::{expand, template};
use univ_model::release::rows::{
    DesignationRow, EntryRow, IdReplacedRow, IdsIssuedRow, MeasurementRow, NameRow, ObjectRow,
    PARAMS, name_key,
};
use univ_model::release::{Row, UncertaintyColumns};
use univ_model::units::Unit;
use univ_model::value::Quantity;

use super::d1::D1Writer;
use super::entries::{Measurements, Numbering, entry_number, entry_row, small};
use crate::crossmatch::ReleaseObject;
use crate::source::{ScaleDomain, SourceDefinition};
use crate::tiles::{domain_of, name_of};

/// `value` as JSON.
///
/// # Errors
///
/// Fails when serde cannot write it as JSON.
pub fn json<T: Serialize + ?Sized>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("a value cannot be written as JSON: {e}"))
}

/// A unit variant's name as serde writes it, such as `galaxy_group`.
#[must_use]
pub fn snake<T: Serialize>(value: &T) -> String {
    super::terms::variant_name(value)
        .unwrap_or_default()
        .to_owned()
}

/// What writing a release's objects counted.
#[derive(Debug, Default)]
pub struct ObjectCounts {
    pub objects: u64,
    pub placed: u64,
    pub entries: u64,
    /// The catalogue entries the release holds from each source.
    pub per_source: BTreeMap<SourceId, u64>,
    /// The placed objects whose derived position the rows do not give back, and the first.
    pub not_kept: u64,
    pub first_not_kept: Option<String>,
}

/// The tables whose keys hold the object id, keyed unique within each object.
const PER_OBJECT: [&str; 4] = [
    EntryRow::TABLE.name,
    MeasurementRow::TABLE.name,
    DesignationRow::TABLE.name,
    NameRow::TABLE.name,
];

/// The rows of a release's objects, as they are written.
pub struct ObjectWriter<'a> {
    pub numbering: Numbering,
    pub counts: ObjectCounts,
    cosmology: &'a Cosmology,
    domains: HashMap<&'a SourceId, ScaleDomain>,
}

/// A preferred measurement as (entry, quantity, n).
fn reference(object: &Object, m: &MeasurementRef) -> (Option<u32>, &'static str, u32) {
    let (quantity, n) = match m {
        MeasurementRef::Distance { index, .. } => ("distance", *index),
        MeasurementRef::Redshift { index, .. } => ("redshift", *index),
        MeasurementRef::Parallax { .. } => ("parallax", 0),
    };
    (entry_number(&object.entries, m.entry()), quantity, small(n))
}

/// A fractional uncertainty as its total, measurement and peculiar-velocity parts.
fn fraction(u: &FractionalUncertainty) -> (Option<f64>, Option<f64>, Option<f64>) {
    match *u {
        FractionalUncertainty::Reported {
            total,
            measurement,
            peculiar_velocity,
        } => (Some(total), measurement, peculiar_velocity),
        FractionalUncertainty::NoneReported => (None, None, None),
    }
}

/// The native value of an object's preferred measurement, as its position derives from it:
/// a distance's or parallax's native value, a redshift's CMB-frame value; and the distance's
/// assumed cosmology.
fn preferred_native(
    object: &Object,
    m: &MeasurementRef,
) -> Option<(Quantity, Option<univ_model::cosmology::AssumedCosmology>)> {
    let entry = &object.entry(m.entry())?.entry;
    match m {
        MeasurementRef::Distance { index, .. } => entry
            .distances
            .get(*index)
            .map(|d| (d.native, d.assumed_cosmology)),
        MeasurementRef::Redshift { index, .. } => {
            entry.redshifts.get(*index).map(|z| (z.cmb.value, None))
        }
        MeasurementRef::Parallax { .. } => entry.parallax.as_deref().map(|p| (p.native, None)),
    }
}

impl<'a> ObjectWriter<'a> {
    /// A writer of release `numbering.release`'s objects, placed under `cosmology`, each in the
    /// scale domain `definitions` give its preferred distance's source.
    #[must_use]
    pub fn new(
        numbering: Numbering,
        definitions: &'a [SourceDefinition],
        cosmology: &'a Cosmology,
    ) -> Self {
        Self {
            numbering,
            counts: ObjectCounts::default(),
            cosmology,
            domains: definitions
                .iter()
                .map(|d| (&d.id, d.scale_domain))
                .collect(),
        }
    }

    /// Writes `o` and all its rows.
    ///
    /// # Errors
    ///
    /// Fails when a value cannot be written as JSON or a row cannot be written.
    pub fn write(&mut self, w: &mut D1Writer<'_>, o: &ReleaseObject) -> Result<(), String> {
        let id = o.id.get();
        let row = self.object_row(o)?;
        self.counts.objects += 1;
        self.counts.placed += u64::from(row.placed);
        w.add(&row)?;
        let object = &o.object;
        for (n, e) in object.entries.iter().enumerate() {
            let entry = entry_row(&mut self.numbering, id, &object.entries, n)?;
            w.add(&entry)?;
            let mut measurements = Measurements {
                writer: w,
                object_id: id,
                entry: entry.entry,
            };
            measurements.write(&mut self.numbering, &e.entry)?;
            self.counts.entries += 1;
            *self
                .counts
                .per_source
                .entry(e.entry.designation.source.clone())
                .or_default() += 1;
        }
        let release = self.numbering.release;
        let designations: BTreeSet<&Designation> = object.designations().collect();
        for d in designations {
            w.add(&DesignationRow {
                release,
                source: self.numbering.sources.n(&d.source),
                identifier: String::from(&*d.identifier),
                object_id: id,
            })?;
        }
        // A name given twice by one source is one row; two spellings of it are two.
        let names: BTreeSet<(&str, &SourceId)> = object
            .names
            .iter()
            .map(|n| (n.name.as_str(), &n.source))
            .collect();
        for (name, source) in names {
            w.add(&NameRow {
                release,
                key: name_key(name),
                object_id: id,
                source: self.numbering.sources.n(source),
                name: name.to_owned(),
            })?;
        }
        w.forget_keys(&PER_OBJECT);
        Ok(())
    }

    /// The scale domain of a placed object: its preferred distance's source's, or, for any
    /// source but a star source, the one its placed distance falls in.
    fn domain(&self, o: &ReleaseObject) -> Option<ScaleDomain> {
        let preferred = o.object.preferred_distance.as_ref()?;
        let source = self.domains.get(&preferred.measurement.entry().source)?;
        Some(domain_of(*source, o.position.as_ref()?.distance().value))
    }

    /// The object's own row, its chain numbered, its derived position checked against it.
    fn object_row(&mut self, o: &ReleaseObject) -> Result<ObjectRow, String> {
        let object = &o.object;
        let domain = self.domain(o);
        let terms = &mut self.numbering.terms;
        let preferred_position = object.preferred_position.as_ref();
        let sky = preferred_position
            .and_then(|p| object.entry(&p.entry))
            .and_then(|e| e.entry.position.as_ref())
            .map(|p| p.icrs);
        let at = o.position.as_ref().and_then(|p| p.cartesian(Frame::Icrs));
        let placed = at.is_some();
        let not_placed = match object.placement {
            Placement::NotPlaced { reason } => Some(terms.of(&reason)?),
            Placement::Placed if placed => None,
            Placement::Placed => Some(terms.id("not_derivable")),
        };
        let preferred = object.preferred_distance.as_ref();
        let domain = domain.filter(|_| placed).map(|d| terms.id(name_of(d)));
        let mut row = ObjectRow {
            release: self.numbering.release,
            id: o.id.get(),
            kind: terms.of(&object.kind)?,
            ra: sky.map(|s| s.ra),
            dec: sky.map(|s| s.dec),
            epoch: sky.map(|s| s.epoch.0),
            position_entry: preferred_position
                .and_then(|p| entry_number(&object.entries, &p.entry)),
            position_class: preferred_position.map(|p| terms.of(&p.class)).transpose()?,
            position_uncertainty_mas: preferred_position.and_then(|p| p.uncertainty_mas),
            placed,
            not_placed,
            scale_domain: domain,
            x: at.map(|p| p[0]),
            y: at.map(|p| p[1]),
            z: at.map(|p| p[2]),
            distance_mpc: None,
            distance_form: None,
            distance_minus: None,
            distance_plus: None,
            distance_level: None,
            measure: None,
            chain: None,
            chain_p1: None,
            chain_p2: None,
            chain_p3: None,
            chain_p4: None,
            preferred_entry: None,
            preferred_quantity: None,
            preferred_n: None,
            preferred_rank: None,
            preferred_mpc: None,
            preferred_decided: None,
            preferred_total: None,
            preferred_measurement: None,
            preferred_velocity: None,
            runner_up_entry: None,
            runner_up_quantity: None,
            runner_up_n: None,
            runner_up_total: None,
            runner_up_measurement: None,
            runner_up_velocity: None,
            links: (!object.links.is_empty())
                .then(|| json(&object.links))
                .transpose()?,
            relations: (!object.relations.is_empty())
                .then(|| json(&object.relations))
                .transpose()?,
        };
        if let Some(p) = preferred {
            let (entry, quantity, n) = reference(object, &p.measurement);
            row.preferred_entry = entry;
            row.preferred_quantity = Some(terms.id(quantity));
            row.preferred_n = Some(n);
            row.preferred_rank = Some(terms.of(&p.rank)?);
            row.preferred_mpc = Some(p.distance_mpc);
            row.preferred_decided = Some(terms.of(&p.decided)?);
            (
                row.preferred_total,
                row.preferred_measurement,
                row.preferred_velocity,
            ) = fraction(&p.uncertainty);
            if let Some((m, u)) = p.runner_up.as_deref() {
                let (entry, quantity, n) = reference(object, m);
                row.runner_up_entry = entry;
                row.runner_up_quantity = Some(terms.id(quantity));
                row.runner_up_n = Some(n);
                (
                    row.runner_up_total,
                    row.runner_up_measurement,
                    row.runner_up_velocity,
                ) = fraction(u);
            }
        }
        if let (Some(position), Some(p)) = (&o.position, preferred) {
            self.derived(&mut row, object, position, &p.measurement)?;
        }
        Ok(row)
    }

    /// Fills the row's derived position, numbering its chain, and checks that the row, its
    /// preferred measurement and the release give the position back.
    fn derived(
        &mut self,
        row: &mut ObjectRow,
        object: &Object,
        position: &DerivedPosition,
        measurement: &MeasurementRef,
    ) -> Result<(), String> {
        let terms = &mut self.numbering.terms;
        let distance = position.distance();
        let u = UncertaintyColumns::from(distance.uncertainty);
        row.distance_mpc = Some(distance.value);
        row.distance_form = Some(terms.id(u.form));
        (row.distance_minus, row.distance_plus, row.distance_level) = (u.minus, u.plus, u.level);
        row.measure = Some(terms.of(&position.measure())?);
        let steps = serde_json::to_value(position.chain()).map_err(|e| e.to_string())?;
        let native = serde_json::to_value(position.native()).map_err(|e| e.to_string())?;
        let t = template(&steps, &native);
        let text = json(&t.steps)?;
        let id = self.numbering.chains.id(row.release, text, t.params.len());
        row.chain = Some(id);
        let mut params = [None; PARAMS];
        for (slot, x) in params.iter_mut().zip(&t.params) {
            *slot = Some(*x);
        }
        [row.chain_p1, row.chain_p2, row.chain_p3, row.chain_p4] = params;

        let direction = position.direction();
        let entry_frame = object
            .preferred_position
            .as_ref()
            .and_then(|p| object.entry(&p.entry))
            .and_then(|e| e.entry.position.as_ref())
            .map(|p| p.native.frame);
        let mut lost = Vec::new();
        if preferred_native(object, measurement)
            != Some((*position.native(), position.assumed().copied()))
        {
            lost.push("its native value or assumed cosmology");
        }
        if position.cosmology() != self.cosmology {
            lost.push("the release cosmology");
        }
        if Some(direction.published_in()) != entry_frame
            || Some(direction.icrs().ra) != row.ra
            || Some(direction.icrs().dec) != row.dec
            || Some(direction.icrs().epoch.0) != row.epoch
        {
            lost.push("its direction");
        }
        if distance.unit != Unit::Megaparsec || u.uncertainty() != Some(distance.uncertainty) {
            lost.push("its distance");
        }
        if expand(&t.steps, &params, &native).as_ref() != Some(&steps) {
            lost.push("its transformation chain");
        }
        if !lost.is_empty() {
            self.counts.not_kept += 1;
            self.counts
                .first_not_kept
                .get_or_insert_with(|| format!("object {}: {}", row.id, lost.join(", ")));
        }
        Ok(())
    }
}

/// The whole id ledger as rows: every release's issued ids and every replacement so far, so a
/// database that never held an earlier release still resolves its ids.
///
/// # Errors
///
/// Fails when a release of the ledger is not numbered, or a value cannot be written as JSON.
pub fn ledger_rows(ledger: &Ledger) -> Result<(Vec<IdsIssuedRow>, Vec<IdReplacedRow>), String> {
    let number = |name: &str| {
        name.parse::<u32>()
            .map_err(|_| format!("the ledger names release {name:?}, which is not a number"))
    };
    let mut issued = Vec::new();
    for r in ledger.releases() {
        if let Some(range) = r.issued {
            issued.push(IdsIssuedRow {
                release: number(&r.name)?,
                first: range.first.get(),
                last: range.last.get(),
            });
        }
    }
    let replaced = ledger
        .replaced()
        .iter()
        .map(|r| {
            let by: Vec<u64> = r.replaced_by.iter().map(|id| id.get()).collect();
            Ok(IdReplacedRow {
                id: r.id.get(),
                release: number(&r.release)?,
                change: snake(&r.change),
                replaced_by: json(&by)?,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok((issued, replaced))
}
