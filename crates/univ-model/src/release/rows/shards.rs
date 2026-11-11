//! The catalogue database's index of a release's object shards (schema version 5): which shard
//! holds each object, the cells each shard holds, and each shard's size.

use serde::{Deserialize, Serialize};

use super::ToSql;
use crate::release::sql::{Row, Scale, Scope, SqlValue, Table};

row! {
    /// The object shard that holds an object's rows (`objects`, `entries`, `measurements`,
    /// `designations` and `object_cells`), so a read by id goes to that shard alone.
    ObjectShardRow => "object_shards", scope: PerRelease, scale: Object, key: ["id", "release"],
    indexes: [], {
        release: u32,
        /// The object's id.
        id: u64,
        /// The shard, from 0: database `OBJECTS_{shard}`.
        shard: u32,
    }
}

row! {
    /// The level-0 cells of one scale domain that one object shard holds: those whose Morton
    /// code ([`crate::release::shards::morton`]) lies from `first` to `last`, both included. A
    /// shard holds one run for each stretch of consecutive cells of one domain it holds, so it
    /// may hold several runs of a domain, between which other shards' cells lie; the runs of
    /// one release never overlap.
    ShardCellRow => "shard_cells", scope: PerRelease, scale: Release,
    key: ["shard", "domain", "first", "release"],
    indexes: [], {
        release: u32,
        shard: u32,
        /// The scale domain, a term.
        domain: u32,
        first: u64,
        last: u64,
    }
}

row! {
    /// One object shard of a release: its objects and the D1 storage its rows are estimated to
    /// take, which publishing sums over the releases not pruned to check the shard's database
    /// against the CON-3 cap.
    ReleaseShardRow => "release_shards", scope: PerRelease, scale: Release, key: ["shard", "release"],
    indexes: [], {
        release: u32,
        shard: u32,
        objects: u64,
        estimated_bytes: u64,
    }
}
