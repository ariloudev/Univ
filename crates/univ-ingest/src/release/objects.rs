//! A release's objects as D1 rows: each object with its preferred measurements and derived
//! position, its catalogue entries, every measurement beside its native value, its
//! designations and common names; and the id ledger.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use univ_model::entry::CatalogueEntry;
use univ_model::ids::{Designation, SourceId};
use univ_model::ledger::Ledger;
use univ_model::object::{Placement, Resolution};
use univ_model::position::Frame;
use univ_model::release::rows::{
    DesignationRow, EntryRow, IdReplacedRow, IdsIssuedRow, MeasurementRow, NameRow, ObjectRow,
    name_key,
};
use univ_model::value::{Quantity, Value};

use crate::crossmatch::ReleaseObject;
use crate::source::SourceDefinition;
use crate::tiles::name_of;

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
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The rows of one release's objects.
#[derive(Debug, Default)]
pub struct ObjectRows {
    pub objects: Vec<ObjectRow>,
    pub entries: Vec<EntryRow>,
    pub measurements: Vec<MeasurementRow>,
    pub designations: Vec<DesignationRow>,
    pub names: Vec<NameRow>,
    /// The catalogue entries the release holds from each source.
    pub per_source: BTreeMap<SourceId, u64>,
}

/// The rows of `objects` in release `release`, each placed object in the scale domain that
/// `definitions` give the source of its preferred distance.
///
/// # Errors
///
/// Fails when a value cannot be written as JSON.
pub fn object_rows(
    release: u32,
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
) -> Result<ObjectRows, String> {
    let domain_of = |source: &SourceId| {
        definitions
            .iter()
            .find(|d| &d.id == source)
            .map(|d| name_of(d.scale_domain).to_owned())
    };
    let mut rows = ObjectRows::default();
    for o in objects {
        let object = &o.object;
        let id = o.id.get();
        let sky = object
            .preferred_position
            .as_ref()
            .and_then(|p| object.entry(&p.entry))
            .and_then(|e| e.entry.position.as_ref())
            .map(|p| p.icrs);
        let at = o.position.as_ref().and_then(|p| p.cartesian(Frame::Icrs));
        let placed = at.is_some();
        let not_placed = match object.placement {
            Placement::NotPlaced { reason } => Some(snake(&reason)),
            Placement::Placed if placed => None,
            Placement::Placed => Some("not_derivable".to_owned()),
        };
        let domain = object
            .preferred_distance
            .as_ref()
            .filter(|_| placed)
            .and_then(|p| domain_of(&p.measurement.entry().source));
        rows.objects.push(ObjectRow {
            release,
            id,
            kind: snake(&object.kind),
            ra: sky.map(|s| s.ra),
            dec: sky.map(|s| s.dec),
            epoch: sky.map(|s| s.epoch.0),
            placed,
            not_placed,
            scale_domain: domain,
            x: at.map(|p| p[0]),
            y: at.map(|p| p[1]),
            z: at.map(|p| p[2]),
            distance_mpc: o.position.as_ref().map(|p| p.distance().value),
            preferred_distance: object.preferred_distance.as_ref().map(json).transpose()?,
            preferred_position: object.preferred_position.as_ref().map(json).transpose()?,
            position: o.position.as_ref().map(json).transpose()?,
            links: json(&object.links)?,
            relations: json(&object.relations)?,
        });
        for e in &object.entries {
            let entry = &e.entry;
            let d = &entry.designation;
            *rows.per_source.entry(d.source.clone()).or_default() += 1;
            rows.entries.push(EntryRow {
                release,
                source: d.source.to_string(),
                identifier: d.identifier.clone(),
                object_id: id,
                anchor: e.resolved_by == Resolution::Anchor,
                resolution: json(&e.resolved_by)?,
                name: entry.name.clone(),
                position: entry.position.as_ref().map(json).transpose()?,
                group_id: entry.group_id.clone(),
                flags: json(&entry.flags)?,
            });
            measurements(release, id, entry, &mut rows.measurements)?;
        }
        let designations: BTreeSet<&Designation> = object.designations.iter().collect();
        for d in designations {
            rows.designations.push(DesignationRow {
                release,
                source: d.source.to_string(),
                identifier: d.identifier.clone(),
                object_id: id,
            });
        }
        // A name given twice by one source is one row; two spellings of it are two.
        let names: BTreeSet<(&str, String)> = object
            .names
            .iter()
            .map(|n| (n.name.as_str(), n.source.to_string()))
            .collect();
        for (name, source) in names {
            rows.names.push(NameRow {
                release,
                key: name_key(name),
                object_id: id,
                source,
                name: name.to_owned(),
            });
        }
    }
    Ok(rows)
}

/// The measurement rows of one entry.
fn measurements(
    release: u32,
    object_id: u64,
    entry: &CatalogueEntry,
    out: &mut Vec<MeasurementRow>,
) -> Result<(), String> {
    let d = &entry.designation;
    let mut push = |quantity: &str,
                    n: usize,
                    kind: String,
                    native: &Quantity,
                    normalized: &Quantity,
                    detail: String|
     -> Result<(), String> {
        out.push(MeasurementRow {
            release,
            object_id,
            source: d.source.to_string(),
            identifier: d.identifier.clone(),
            quantity: quantity.to_owned(),
            n: u32::try_from(n).unwrap_or(u32::MAX),
            kind_of_value: kind,
            native_value: native.value,
            native_unit: snake(&native.unit),
            value: normalized.value,
            unit: snake(&normalized.unit),
            uncertainty: json(&native.uncertainty)?,
            detail,
        });
        Ok(())
    };
    for (n, distance) in entry.distances.iter().enumerate() {
        let kind = snake(&distance.kind_of_value);
        push(
            "distance",
            n,
            kind,
            &distance.native,
            &distance.normalized,
            json(distance)?,
        )?;
    }
    for (n, z) in entry.redshifts.iter().enumerate() {
        // As published: the velocity cz where the source gives one, else z; normalized to the
        // heliocentric z, whose CMB-frame value the detail gives.
        let native = z.velocity.as_ref().unwrap_or(&z.cmb.heliocentric);
        let kind = snake(&z.kind_of_value);
        push("redshift", n, kind, native, &z.cmb.heliocentric, json(z)?)?;
    }
    let pm = entry.proper_motion.as_ref();
    let values: [(&str, Option<&Value>); 5] = [
        ("parallax", entry.parallax.as_ref()),
        ("velocity", entry.velocity.as_ref()),
        ("proper_motion_ra", pm.map(|p| &p.ra)),
        ("proper_motion_dec", pm.map(|p| &p.dec)),
        ("magnitude", entry.magnitude.as_ref().map(|m| &m.value)),
    ];
    for (quantity, value) in values {
        if let Some(v) = value {
            let detail = if quantity == "magnitude" {
                json(&entry.magnitude)?
            } else {
                json(v)?
            };
            push(
                quantity,
                0,
                snake(&v.kind),
                &v.native,
                &v.normalized,
                detail,
            )?;
        }
    }
    Ok(())
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
