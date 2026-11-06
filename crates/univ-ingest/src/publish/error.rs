//! Why a publish did not move `latest` to the release.

use std::fmt;

use univ_model::release::D1_CAP_BYTES;
use univ_model::release::shards::Database;

use super::Latest;
use crate::release::decimal_bytes;

/// A publish that did not move `latest` to the release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishError {
    /// The build output holds no complete release of that number.
    NotBuilt { number: u32, problem: String },
    /// The release is already published, and is left as it is.
    AlreadyPublished {
        number: u32,
        published_at: String,
        latest: Latest,
    },
    /// The release was pruned.
    Pruned { number: u32, pruned_at: String },
    /// `latest` points to a newer release.
    NotNewer { number: u32, latest: u32 },
    /// The release holds only what changed from its parent, which `latest` does not point to.
    NotOnLatest {
        number: u32,
        parent: u32,
        latest: Option<u32>,
    },
    /// The release's rows and those of the releases not pruned are estimated beyond the cap in
    /// one database.
    OverCap {
        number: u32,
        database: Database,
        /// The release's own estimate, bytes.
        this: u64,
        /// The estimates of the other releases not pruned, summed, bytes.
        others: u64,
        /// How many releases those are.
        releases: u64,
    },
    /// The release needs more object shards than Univ Server binds.
    TooManyShards { number: u32, needs: u32, binds: u32 },
    /// A database holds releases of a schema older than the release's, whose rows are not
    /// shared: they must be brought under it before anything is published.
    OlderSchema {
        number: u32,
        database: String,
        holds: u32,
        needs: u32,
    },
    /// The journal cannot be used.
    Journal { number: u32, problem: String },
    /// An upload or check failed part way; the next run continues from the journal.
    Interrupted {
        number: u32,
        latest: Latest,
        done: usize,
        total: usize,
        problem: String,
    },
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBuilt { number, problem } => {
                write!(f, "release {number} cannot be published: {problem}")
            }
            Self::AlreadyPublished {
                number,
                published_at,
                latest,
            } => write!(
                f,
                "release {number} was already published at {published_at}, and is left as it \
                 is; latest points to {latest}"
            ),
            Self::Pruned { number, pruned_at } => write!(
                f,
                "release {number} was pruned at {pruned_at} and is not published again"
            ),
            Self::NotNewer { number, latest } => write!(
                f,
                "release {number} is not published: latest points to release {latest}, which \
                 is newer"
            ),
            Self::NotOnLatest {
                number,
                parent,
                latest,
            } => write!(
                f,
                "release {number} is not published: it holds only what changed from release \
                 {parent}, and latest points to {}; publish release {parent} first, or build \
                 release {number} again on the release latest points to",
                latest.map_or_else(|| "no release".to_owned(), |l| format!("release {l}"))
            ),
            Self::OverCap {
                number,
                database,
                this,
                others,
                releases,
            } => write!(
                f,
                "release {number} is not published: its rows in {database} are estimated at \
                 {this} bytes and those of the {releases} release(s) not pruned at {others} \
                 bytes, together {} bytes, beyond the {} cap of {D1_CAP_BYTES} bytes; prune a \
                 release first",
                this + others,
                decimal_bytes(D1_CAP_BYTES)
            ),
            Self::TooManyShards {
                number,
                needs,
                binds,
            } => write!(
                f,
                "release {number} is not published: it needs {needs} object shards and Univ \
                 Server binds {binds}; bind more in both wrangler.toml files, or build it with a \
                 larger shard budget"
            ),
            Self::OlderSchema {
                number,
                database,
                holds,
                needs,
            } => write!(
                f,
                "release {number} is not published: {database} holds releases of D1 schema \
                 version {holds}, whose rows each belong to one release, and this release is of \
                 version {needs}, whose rows are shared between releases; nothing was uploaded"
            ),
            Self::Journal { number, problem } => {
                write!(f, "release {number} is not published: {problem}")
            }
            Self::Interrupted {
                number,
                latest,
                done,
                total,
                problem,
            } => write!(
                f,
                "release {number} is not completely published: {problem}; latest points to \
                 {latest}; {done} of {total} files are uploaded and checked, and publishing \
                 release {number} again continues from there"
            ),
        }
    }
}

impl std::error::Error for PublishError {}
