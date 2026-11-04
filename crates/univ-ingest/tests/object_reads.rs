//! Univ Server's object lookup (look-up-an-object), read back from the fixture release's
//! compact D1 rows with univ-model's decoder: each object's rows are handed over as D1 gives
//! them, JSON numbers, strings and nulls, and the record they decode to is checked against the
//! cross-matched object it was written from.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers unwrap and panic: a failure there is the test failing"
)]

mod common;
mod object_reads_support;
mod release_fixture;

use object_reads_support::{Decoded, decoded, key_of, no_duplicate_keys};
use univ_model::api::objects::{MeasuredQuantity, MeasurementRecord};
use univ_model::object::{NotPlacedReason, Placement};
use univ_model::position::{BARYCENTRE, Frame, ICRS_CARTESIAN_NAME};
use univ_model::units::Unit;
use univ_model::value::Quantity;

/// The record's measurement (entry, quantity, n).
fn find(
    measurements: &[MeasurementRecord],
    entry: u32,
    quantity: MeasuredQuantity,
    n: u32,
) -> &MeasurementRecord {
    measurements
        .iter()
        .find(|m| (m.entry, m.quantity, m.n) == (entry, quantity, n))
        .unwrap_or_else(|| panic!("no measurement {entry} {quantity:?} {n}"))
}

#[test]
fn every_catalogue_entry_reads_with_its_source_designation_and_resolution() {
    // AC-131.
    let Decoded { pairs } = decoded();
    let mut types = 0;
    for (o, r) in pairs {
        assert_eq!(r.entries.len(), o.object.entries.len(), "{}", o.id.get());
        // A Hipparcos catalogue entry attaches to the Hipparcos-2 star of the same HIP number.
        for e in r.entries.iter().filter(|e| e.source == "hipparcos-1997") {
            assert!(
                r.entries
                    .iter()
                    .any(|s| s.source == "hipparcos-2007" && s.identifier == e.identifier),
                "HIP {} in object {} with no Hipparcos-2 entry",
                e.identifier,
                o.id.get()
            );
        }
        for ((n, e), read) in (0u32..).zip(&o.object.entries).zip(&r.entries) {
            let entry = &e.entry;
            assert_eq!(read.entry, n);
            assert_eq!(read.source, entry.designation.source.as_str());
            assert_eq!(read.identifier, entry.designation.identifier);
            assert_eq!(read.resolved_by, e.resolved_by, "{}", o.id.get());
            assert_eq!(read.name, entry.name);
            assert_eq!(read.group_id, entry.group_id);
            assert_eq!(read.flags, entry.flags);
            assert_eq!(
                read.spectral_type
                    .as_ref()
                    .map(|t| (t.text.as_str(), t.kind_of_value)),
                entry
                    .spectral_type
                    .as_ref()
                    .map(|t| (t.text.as_str(), t.kind)),
                "{}",
                o.id.get()
            );
            types += usize::from(read.spectral_type.is_some());
            match (&read.position, &entry.position) {
                (None, None) => {}
                (Some(p), Some(q)) => {
                    assert_eq!(p.frame, q.native.frame);
                    assert_eq!((p.lon, p.lat), (q.native.lon, q.native.lat));
                    assert_eq!((p.unit, p.epoch), (q.native.unit, q.native.epoch.0));
                    assert_eq!((p.ra, p.dec), (q.icrs.ra, q.icrs.dec));
                    assert_eq!(p.uncertainty, q.uncertainty);
                }
                other => panic!("{other:?}"),
            }
        }
        // Every designation is an entry's own, or one its source gives beside it from a
        // field of the entry, such as the HIP number a Gaia–Hipparcos entry names.
        for d in &o.object.designations {
            let own = r
                .entries
                .iter()
                .any(|e| e.source == d.source.as_str() && e.identifier == d.identifier);
            let beside = r.entries.iter().any(|e| {
                e.source == d.source.as_str()
                    && [&e.name, &e.group_id]
                        .into_iter()
                        .flatten()
                        .any(|f| d.identifier.ends_with(f.as_str()))
            });
            assert!(own || beside, "{d:?}");
        }
        // The record's designations are the object's `designations` rows, each once, in order.
        let rows: std::collections::BTreeSet<_> = o.object.designations.iter().collect();
        assert!(
            r.designations.iter().eq(rows.into_iter()),
            "{}: {:?}",
            o.id.get(),
            r.designations
        );
    }
    assert_eq!(
        types, 5,
        "the five Hipparcos catalogue entries read with their spectral types"
    );
}

#[test]
fn every_measurement_has_its_native_value_unit_uncertainty_kind_and_source() {
    // AC-132.
    let Decoded { pairs } = decoded();
    let mut redshifts = 0;
    let mut colours = 0;
    for (o, r) in pairs {
        let mut count = 0;
        for (e, entry) in (0u32..).zip(&o.object.entries) {
            let entry = &entry.entry;
            let source = entry.designation.source.as_str();
            let check = |m: &MeasurementRecord, native: &Quantity, normalized: &Quantity, kind| {
                assert_eq!(&m.native, native, "{}", o.id.get());
                assert_eq!(m.normalized.as_ref().unwrap_or(&m.native), normalized);
                assert_eq!(m.kind_of_value, kind);
                assert_eq!(m.source, source);
            };
            for (n, d) in (0u32..).zip(&entry.distances) {
                let m = find(&r.measurements, e, MeasuredQuantity::Distance, n);
                check(m, &d.native, &d.normalized, d.kind_of_value);
                let detail = m.distance.as_ref().unwrap();
                assert_eq!((detail.kind, &detail.method), (d.kind, &d.method));
                assert_eq!(detail.measure, d.measure);
                assert_eq!(detail.also_published, d.also_published);
                assert_eq!(detail.assumed_cosmology, d.assumed_cosmology);
                count += 1;
            }
            for (n, z) in (0u32..).zip(&entry.redshifts) {
                let m = find(&r.measurements, e, MeasuredQuantity::Redshift, n);
                let published = z.cmb.as_published();
                check(
                    m,
                    z.velocity.as_ref().unwrap_or(&published),
                    &published,
                    z.kind_of_value,
                );
                let detail = m.redshift.as_ref().unwrap();
                assert_eq!(detail.cmb, z.cmb.value, "the CMB-frame redshift");
                assert_eq!(detail.cmb.unit, Unit::Dimensionless);
                assert_eq!(
                    (detail.kind, detail.published_frame),
                    (z.kind, z.cmb.published)
                );
                assert_eq!(detail.dipole, z.cmb.dipole);
                count += 1;
                redshifts += 1;
            }
            let pm = entry.proper_motion.as_ref();
            let sgv = entry.supergalactic_velocity.as_ref();
            let values = [
                (MeasuredQuantity::Parallax, entry.parallax.as_ref()),
                (MeasuredQuantity::Velocity, entry.velocity.as_ref()),
                (MeasuredQuantity::ProperMotionRa, pm.map(|p| &p.ra)),
                (MeasuredQuantity::ProperMotionDec, pm.map(|p| &p.dec)),
                (MeasuredQuantity::SupergalacticVelocityX, sgv.map(|v| &v.x)),
                (MeasuredQuantity::SupergalacticVelocityY, sgv.map(|v| &v.y)),
                (MeasuredQuantity::SupergalacticVelocityZ, sgv.map(|v| &v.z)),
                (
                    MeasuredQuantity::Magnitude,
                    entry.magnitude.as_ref().map(|m| &m.value),
                ),
            ];
            for (quantity, value) in values {
                if let Some(v) = value {
                    let m = find(&r.measurements, e, quantity, 0);
                    check(m, &v.native, &v.normalized, v.kind);
                    count += 1;
                }
            }
            if let Some(mag) = &entry.magnitude {
                let m = find(&r.measurements, e, MeasuredQuantity::Magnitude, 0);
                assert_eq!(m.band.as_deref(), Some(mag.band.as_str()));
            }
            for (n, c) in (0u32..).zip(&entry.colours) {
                let m = find(&r.measurements, e, MeasuredQuantity::ColourIndex, n);
                check(m, &c.value.native, &c.value.normalized, c.value.kind);
                assert_eq!(m.band.as_deref(), Some(c.band_pair().as_str()));
                assert_eq!(m.derived_from, c.derived_from);
                colours += 1;
                count += 1;
            }
        }
        assert_eq!(r.measurements.len(), count, "{}", o.id.get());
    }
    assert!(redshifts > 0);
    assert!(colours > 0, "the fixture release reads colour indices back");
}

#[test]
fn the_preferred_measurement_reads_with_the_ranking_that_chose_it() {
    // AC-133.
    let Decoded { pairs } = decoded();
    let mut runners_up = 0;
    for (o, r) in pairs {
        let Some(p) = &o.object.preferred_distance else {
            assert_eq!(r.preferred, None, "{}", o.id.get());
            continue;
        };
        let read = r.preferred.as_ref().unwrap();
        assert_eq!(read.measurement, key_of(&o.object, &p.measurement));
        assert_eq!((read.rank, read.decided), (p.rank, p.decided));
        assert_eq!(read.distance_mpc, p.distance_mpc);
        assert_eq!(read.uncertainty, p.uncertainty);
        match (&read.runner_up, &p.runner_up) {
            (None, None) => {}
            (Some(read), Some((m, u))) => {
                assert_eq!(read.measurement, key_of(&o.object, m));
                assert_eq!(&read.uncertainty, u);
                runners_up += 1;
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(runners_up > 0);
}

#[test]
fn every_derived_position_names_its_frame_epoch_cosmology_and_chain() {
    // AC-134.
    let Decoded { pairs } = decoded();
    let mut units = std::collections::BTreeSet::new();
    for (o, r) in pairs {
        let Some(position) = &o.position else {
            continue;
        };
        let read = r.position.as_ref().unwrap();
        assert_eq!(read.chain, position.chain(), "{}", o.id.get());
        let factor = Unit::Megaparsec.factor_to(read.unit).unwrap();
        let at = position.cartesian(Frame::Icrs).unwrap();
        assert_eq!([read.x, read.y, read.z], at.map(|c| c * factor));
        assert_eq!(read.unit == Unit::Parsec, read.scale_domain == "stars");
        assert_eq!(
            (read.frame.as_str(), read.origin.as_str()),
            (ICRS_CARTESIAN_NAME, BARYCENTRE)
        );
        let direction = position.direction();
        assert_eq!(read.epoch, direction.icrs().epoch.0);
        assert_eq!(read.direction_frame, direction.published_in());
        assert_eq!(&read.cosmology, position.cosmology());
        assert_eq!(&read.distance, position.distance());
        assert_eq!(read.measure, position.measure());
        assert_eq!(&read.native, position.native());
        assert_eq!(read.assumed_cosmology.as_ref(), position.assumed());
        assert_eq!(r.not_placed, None);
        units.insert(format!("{:?}", read.unit));
    }
    assert_eq!(
        units.len(),
        2,
        "stars in parsecs, galaxies in megaparsecs: {units:?}"
    );
}

#[test]
fn every_candidate_match_reads_with_its_confidence() {
    // AC-135.
    let Decoded { pairs } = decoded();
    let mut links = 0;
    for (o, r) in pairs {
        assert_eq!(r.links, o.object.links, "{}", o.id.get());
        assert_eq!(r.relations, o.object.relations, "{}", o.id.get());
        for (read, link) in r.links.iter().zip(&o.object.links) {
            assert_eq!(read.confidence, link.confidence);
            links += 1;
        }
    }
    assert!(links > 0);
}

#[test]
fn an_object_with_no_distance_has_a_sky_position_and_no_derived_position() {
    // AC-136.
    let Decoded { pairs } = decoded();
    let mut unplaced = 0;
    for (o, r) in pairs {
        if o.position.is_some() {
            continue;
        }
        unplaced += 1;
        assert_eq!(r.position, None);
        let reason = match o.object.placement {
            Placement::NotPlaced { reason } => reason,
            Placement::Placed => NotPlacedReason::NotDerivable,
        };
        assert_eq!(r.not_placed, Some(reason), "{}", o.id.get());
        let Some(p) = &o.object.preferred_position else {
            assert_eq!(r.sky_position, None);
            continue;
        };
        let sky = r.sky_position.as_ref().unwrap();
        let icrs = o
            .object
            .entry(&p.entry)
            .unwrap()
            .entry
            .position
            .as_ref()
            .unwrap()
            .icrs;
        assert_eq!(sky.frame, Frame::Icrs);
        assert_eq!(
            (sky.ra, sky.dec, sky.epoch),
            (icrs.ra, icrs.dec, icrs.epoch.0)
        );
        let entry = &r.entries[sky.entry as usize];
        assert_eq!(entry.identifier, p.entry.identifier);
        assert_eq!(
            (sky.class, sky.uncertainty_mas),
            (p.class, p.uncertainty_mas)
        );
    }
    assert!(unplaced > 0);
}

#[test]
fn candidate_links_are_written_with_no_duplicate_key() {
    let texts = object_reads_support::link_texts();
    assert_eq!(texts.len(), 2, "the two objects given links");
    for text in &texts {
        no_duplicate_keys(text).unwrap();
    }
    assert!(
        texts
            .iter()
            .any(|t| t.contains(r#""by":"match","method":"#)),
        "{texts:?}"
    );
    // The shape written before: the tag `method` and a match's `method` in one object.
    let old = r#"[{"reason":{"method":"match","method":"position"}}]"#;
    assert!(no_duplicate_keys(old).is_err());
}
