//! The rows of a release's objects: each object, the distinct transformation chains of their
//! derived positions, their catalogue entries, every measurement, their designations and
//! common names. The schema says how each column holds its value.

use serde::{Deserialize, Serialize};

use super::{Json, ToSql};
use crate::release::sql::{Row, Scale, Scope, SqlValue, Table};

/// The per-object parameters a chain can take: `chain_p1` to `chain_p4`.
pub const PARAMS: usize = 4;

row! {
    /// A distinct transformation chain, its per-object numbers left as `"$1"` to `"$4"` and its
    /// native value as `"$native"` (see [`crate::release::chains`]).
    ChainRow => "chains", scope: PerRelease, scale: Chain, key: ["id", "release"], indexes: [], {
        release: u32,
        id: u32,
        /// JSON array of the chain's steps.
        steps: Json,
        /// How many per-object parameters it takes.
        params: u32,
        /// How many objects of the release it derived.
        objects: u64,
    }
}

row! {
    /// An object with its preferred measurements and, when placed, its derived position. Term
    /// columns are `u32`; entries are numbered by their place among the object's.
    ObjectRow => "objects", scope: PerRelease, scale: Object, key: ["id", "release"], indexes: [], {
        release: u32,
        id: u64,
        kind: u32,
        /// Its preferred sky position, ICRS degrees, at `epoch` (a Julian year).
        ra: Option<f64>,
        dec: Option<f64>,
        epoch: Option<f64>,
        position_entry: Option<u32>,
        position_class: Option<u32>,
        position_uncertainty_mas: Option<f64>,
        placed: bool,
        /// Why it is not placed, when it is not.
        not_placed: Option<u32>,
        /// The scale domain of its preferred distance's source, when placed.
        scale_domain: Option<u32>,
        /// ICRS Cartesian, Mpc, under the release cosmology.
        x: Option<f64>,
        y: Option<f64>,
        z: Option<f64>,
        distance_mpc: Option<f64>,
        distance_form: Option<u32>,
        distance_minus: Option<f64>,
        distance_plus: Option<f64>,
        distance_level: Option<f64>,
        measure: Option<u32>,
        chain: Option<u32>,
        chain_p1: Option<f64>,
        chain_p2: Option<f64>,
        chain_p3: Option<f64>,
        chain_p4: Option<f64>,
        preferred_entry: Option<u32>,
        preferred_quantity: Option<u32>,
        preferred_n: Option<u32>,
        preferred_rank: Option<u32>,
        preferred_mpc: Option<f64>,
        preferred_decided: Option<u32>,
        preferred_total: Option<f64>,
        preferred_measurement: Option<f64>,
        preferred_velocity: Option<f64>,
        runner_up_entry: Option<u32>,
        runner_up_quantity: Option<u32>,
        runner_up_n: Option<u32>,
        runner_up_total: Option<f64>,
        runner_up_measurement: Option<f64>,
        runner_up_velocity: Option<f64>,
    }
}

row! {
    /// A catalogue entry of an object: the object's entry number `entry`.
    EntryRow => "entries", scope: PerRelease, scale: Entry, key: ["object_id", "entry", "release"],
    indexes: [], {
        release: u32,
        object_id: u64,
        entry: u32,
        source: u32,
        identifier: String,
        resolution: u32,
        scheme: Option<u32>,
        shared: Option<String>,
        via_entry: Option<u32>,
        confidence: Option<f64>,
        separation_arcsec: Option<f64>,
        difference: Option<u32>,
        delta: Option<f64>,
        name: Option<String>,
        frame: Option<u32>,
        lon: Option<f64>,
        lat: Option<f64>,
        position_unit: Option<u32>,
        epoch: Option<f64>,
        ra: Option<f64>,
        dec: Option<f64>,
        error_ra: Option<f64>,
        error_dec: Option<f64>,
        error_unit: Option<u32>,
        error_kind: Option<u32>,
        group_id: Option<String>,
        /// The source (its `n`) and identifier of the designation it was resolved through,
        /// where that is none of the object's entries.
        via_source: Option<u32>,
        via_identifier: Option<String>,
        /// Its spectral type as published, and that type's kind of value (a term).
        spectral_type: Option<String>,
        spectral_type_kind: Option<u32>,
    }
}

row! {
    /// A measurement of an entry: `quantity` (a term) is `distance`, `redshift`, `parallax`,
    /// `velocity`, `proper_motion_ra`, `proper_motion_dec`, `magnitude` or
    /// `supergalactic_velocity_x`, `_y` or `_z`, `n` its place among the entry's values of
    /// that quantity.
    MeasurementRow => "measurements", scope: PerRelease, scale: Entry,
    key: ["object_id", "entry", "quantity", "n", "release"], indexes: [], {
        release: u32,
        object_id: u64,
        entry: u32,
        quantity: u32,
        n: u32,
        kind_of_value: u32,
        native_value: f64,
        native_unit: u32,
        native_form: u32,
        native_minus: Option<f64>,
        native_plus: Option<f64>,
        native_level: Option<f64>,
        /// The normalized value, none when it is the native one unchanged.
        value: Option<f64>,
        unit: Option<u32>,
        form: Option<u32>,
        minus: Option<f64>,
        plus: Option<f64>,
        level: Option<f64>,
        kind: Option<u32>,
        method: Option<u32>,
        measure: Option<u32>,
        cmb: Option<f64>,
        cmb_form: Option<u32>,
        cmb_minus: Option<f64>,
        cmb_plus: Option<f64>,
        cmb_level: Option<f64>,
        /// The frame a redshift was published in (a term), none for heliocentric.
        published_frame: Option<u32>,
    }
}

row! {
    /// What a measurement holds beside its values, for the few that hold any: a distance's
    /// value as also published and the cosmology it assumes, a colour index's two magnitude
    /// columns.
    MeasurementDetailRow => "measurement_details", scope: PerRelease, scale: Entry,
    key: ["object_id", "entry", "quantity", "n", "release"], indexes: [], {
        release: u32,
        object_id: u64,
        entry: u32,
        quantity: u32,
        n: u32,
        also_value: Option<f64>,
        also_unit: Option<u32>,
        also_form: Option<u32>,
        also_minus: Option<f64>,
        also_plus: Option<f64>,
        also_level: Option<f64>,
        assumed_hubble_constant: Option<f64>,
        assumed_omega_matter: Option<f64>,
        assumed_omega_lambda: Option<f64>,
        derived_from_first: Option<String>,
        derived_from_second: Option<String>,
    }
}

row! {
    /// A flag of a catalogue entry: its source's column (a term) and the value as published.
    EntryFlagRow => "entry_flags", scope: PerRelease, scale: Entry,
    key: ["object_id", "entry", "flag", "release"], indexes: [["flag", "value", "release"]], {
        release: u32,
        object_id: u64,
        entry: u32,
        flag: u32,
        value: String,
    }
}

row! {
    /// A paper a researched entry's values come from, `n` its place in the entry's list.
    EntryReferenceRow => "entry_references", scope: PerRelease, scale: Entry,
    key: ["object_id", "entry", "n", "release"], indexes: [], {
        release: u32,
        object_id: u64,
        entry: u32,
        n: u32,
        citation: String,
        doi: Option<String>,
        bibcode: Option<String>,
    }
}

row! {
    /// A candidate match of an object, `n` its place among them, with its reason in the columns
    /// of an entry's resolution.
    ObjectLinkRow => "object_links", scope: PerRelease, scale: Object,
    key: ["object_id", "n", "release"], indexes: [], {
        release: u32,
        object_id: u64,
        n: u32,
        entry_source: u32,
        entry_identifier: String,
        candidate_source: u32,
        candidate_identifier: String,
        resolution: u32,
        scheme: Option<u32>,
        shared: Option<String>,
        via_source: Option<u32>,
        via_identifier: Option<String>,
        separation_arcsec: Option<f64>,
        difference: Option<u32>,
        delta: Option<f64>,
        reason_confidence: Option<f64>,
        confidence: f64,
    }
}

row! {
    /// A relation of an object to another, `n` its place among them.
    ObjectRelationRow => "object_relations", scope: PerRelease, scale: Object,
    key: ["object_id", "n", "release"], indexes: [], {
        release: u32,
        object_id: u64,
        n: u32,
        kind: u32,
        entry_source: u32,
        entry_identifier: String,
        other_source: u32,
        other_identifier: String,
        scheme: String,
        identifier: String,
    }
}

row! {
    /// An object that its source's own column assigns to a researched structure.
    MembershipRow => "memberships", scope: PerRelease, scale: Object,
    key: ["object_id", "structure", "release"], indexes: [["structure", "release"]], {
        release: u32,
        object_id: u64,
        structure: String,
        source: u32,
        value: String,
    }
}

row! {
    /// A designation an object carries.
    DesignationRow => "designations", scope: PerRelease, scale: Entry,
    key: ["source", "identifier", "object_id", "release"], indexes: [["object_id", "release"]], {
        release: u32,
        source: u32,
        identifier: String,
        object_id: u64,
    }
}

row! {
    /// A common name of an object, by its key (see [`super::name_key`]). The name itself is
    /// part of the primary key: two spellings an object's source gives, such as `M 31` and
    /// `M31`, share a key and are both kept.
    NameRow => "names", scope: PerRelease, scale: Object,
    key: ["key", "object_id", "source", "name", "release"],
    indexes: [["object_id", "release"]], {
        release: u32,
        key: String,
        object_id: u64,
        source: u32,
        name: String,
    }
}
