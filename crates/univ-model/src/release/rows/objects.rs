//! The rows of a release's objects: each object, the distinct transformation chains of their
//! derived positions, their catalogue entries, every measurement, their designations and
//! common names. The schema says how each column holds its value.

use serde::{Deserialize, Serialize};

use super::ToSql;
use crate::release::sql::{Row, Scope, SqlValue, Table};

/// The per-object parameters a chain can take: `chain_p1` to `chain_p4`.
pub const PARAMS: usize = 4;

row! {
    /// A distinct transformation chain, its per-object numbers left as `"$1"` to `"$4"` and its
    /// native value as `"$native"` (see [`crate::release::chains`]).
    ChainRow => "chains", scope: PerRelease, key: ["release", "id"], indexes: [], {
        release: u32,
        id: u32,
        /// JSON array of the chain's steps.
        steps: String,
        /// How many per-object parameters it takes.
        params: u32,
        /// How many objects of the release it derived.
        objects: u64,
    }
}

row! {
    /// An object with its preferred measurements and, when placed, its derived position. Term
    /// columns are `u32`; entries are numbered by their place among the object's.
    ObjectRow => "objects", scope: PerRelease, key: ["release", "id"], indexes: [], {
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
        /// JSON array of its candidate links, none when it has none.
        links: Option<String>,
        /// JSON array of its relations, none when it has none.
        relations: Option<String>,
    }
}

row! {
    /// A catalogue entry of an object: the object's entry number `entry`.
    EntryRow => "entries", scope: PerRelease, key: ["release", "object_id", "entry"],
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
        /// JSON object of its flags, none when it has none.
        flags: Option<String>,
        /// JSON object of what no column holds, none when nothing.
        extra: Option<String>,
    }
}

row! {
    /// A measurement of an entry: `quantity` (a term) is `distance`, `redshift`, `parallax`,
    /// `velocity`, `proper_motion_ra`, `proper_motion_dec`, `magnitude` or
    /// `supergalactic_velocity_x`, `_y` or `_z`, `n` its place among the entry's values of
    /// that quantity.
    MeasurementRow => "measurements", scope: PerRelease,
    key: ["release", "object_id", "entry", "quantity", "n"], indexes: [], {
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
        /// JSON object of what no column holds, none when nothing.
        extra: Option<String>,
    }
}

row! {
    /// A designation an object carries.
    DesignationRow => "designations", scope: PerRelease,
    key: ["release", "source", "identifier", "object_id"], indexes: [["release", "object_id"]], {
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
    NameRow => "names", scope: PerRelease,
    key: ["release", "key", "object_id", "source", "name"],
    indexes: [], {
        release: u32,
        key: String,
        object_id: u64,
        source: u32,
        name: String,
    }
}
