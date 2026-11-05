//! How a source is reached: a provider's services under each mirror, files fetched whole, or
//! files kept in this repository, and the checks of each.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How a source is fetched from each mirror.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "method", deny_unknown_fields)]
pub enum Access {
    /// A `VizieR` table through `asu-tsv`, with these constraints. A large table is fetched in
    /// `slices` of an integer column. `rows` is the number of rows the whole query must give,
    /// such as the count a `ReadMe` states for an unfiltered table, checked once every slice is
    /// read. The computed columns `_RAJ2000` and `_DEJ2000` give decimal degrees.
    VizierTsv {
        table: String,
        constraints: Vec<String>,
        #[serde(default)]
        slices: Option<Slices>,
        #[serde(default)]
        rows: Option<u64>,
    },
    /// An SQL query to `SkyServer`, split into `chunks` by `{chunk}` and `{chunks}` in the query.
    /// `count` is a query for the number of rows the chunks must hold between them: the service
    /// stops at 500,000 rows without saying so.
    SkyserverSql {
        query: String,
        chunks: u32,
        #[serde(default)]
        count: Option<String>,
    },
    /// An ADQL query to a TAP service, split into `chunks` by `{chunk}` and `{chunks}` in the
    /// query. `count` is a query for the number of rows the chunks must hold between them: the
    /// ESA Gaia archive stops anonymous answers at 50,000 rows without saying so in CSV. `run`
    /// says whether each chunk is answered at once (`sync`) or run as a job (`async`), for a
    /// query the service cannot answer within a request, such as a join of two large tables;
    /// the count is always answered at once.
    Tap {
        query: String,
        chunks: u32,
        #[serde(default)]
        count: Option<String>,
        #[serde(default)]
        run: TapRun,
    },
    /// Files under each mirror, each fetched whole and resumable.
    Files { files: Vec<RemoteFile> },
    /// Files kept in this repository under `path`, such as the researched structures, which are
    /// read from their papers rather than fetched.
    Repository {
        path: String,
        /// What its files are: structure files, or star files.
        #[serde(default, skip_serializing_if = "Holds::is_structures")]
        holds: Holds,
    },
}

/// What the files of a source kept in the repository are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Holds {
    /// Researched structures, one structure file each.
    #[default]
    Structures,
    /// Researched star distances, one star file each.
    StarDistances,
}

impl Access {
    /// What a source kept in this repository holds, and the path it is kept under; none for a
    /// source that is fetched.
    #[must_use]
    pub fn repository(&self) -> Option<(Holds, &str)> {
        match self {
            Self::Repository { path, holds } => Some((*holds, path)),
            _ => None,
        }
    }
}

impl Holds {
    /// Whether the files are structure files, the default, which a definition need not say.
    #[must_use]
    pub fn is_structures(&self) -> bool {
        *self == Self::Structures
    }
}

/// How a TAP service is asked a query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapRun {
    /// Answered within the request, at `/sync`.
    #[default]
    Sync,
    /// Run as a job at `/async`, polled until it completes, its result then fetched.
    Async,
}

/// A file under each mirror, with the size and SHA-256 its provider publishes, where it does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteFile {
    pub path: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Inclusive ranges `from..from+step-1`, … up to `to`, of an integer column: they meet without
/// gap or overlap, as a range of a real-valued column could not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slices {
    pub column: String,
    pub from: i64,
    pub to: i64,
    pub step: i64,
}

impl Access {
    pub(super) fn check(&self) -> Result<(), (String, String)> {
        let problem = match self {
            Self::VizierTsv { table, .. } if table.trim().is_empty() => {
                Some(("access.table", "is empty"))
            }
            Self::SkyserverSql { query, .. } | Self::Tap { query, .. }
                if query.trim().is_empty() =>
            {
                Some(("access.query", "is empty"))
            }
            Self::SkyserverSql { query, chunks, .. } | Self::Tap { query, chunks, .. }
                if *chunks > 1 && !query.contains("{chunk}") =>
            {
                Some(("access.query", "is split into chunks but holds no {chunk}"))
            }
            Self::VizierTsv {
                slices: Some(s), ..
            } if s.step < 1 || s.to < s.from => {
                Some(("access.slices", "needs a positive step and to ≥ from"))
            }
            Self::SkyserverSql { chunks: 0, .. } | Self::Tap { chunks: 0, .. } => {
                Some(("access.chunks", "is zero"))
            }
            Self::Files { files } if files.is_empty() => Some(("access.files", "lists no file")),
            Self::Files { files } if files.iter().any(|f| f.path.trim().is_empty()) => {
                Some(("access.files", "holds a file with an empty path"))
            }
            Self::Files { files }
                if files.iter().any(|f| {
                    f.sha256
                        .as_ref()
                        .is_some_and(|d| d.len() != 64 || !d.bytes().all(|b| b.is_ascii_hexdigit()))
                }) =>
            {
                Some((
                    "access.files",
                    "holds a SHA-256 that is not 64 hexadecimal digits",
                ))
            }
            Self::Files { files } if !distinct_names(files) => Some((
                "access.files",
                "holds two files with the same file name, which would share one file on disk",
            )),
            Self::Repository { path, .. } if path.trim().is_empty() => {
                Some(("access.path", "is empty"))
            }
            _ => None,
        };
        problem.map_or(Ok(()), |(field, what)| {
            Err((field.to_owned(), what.to_owned()))
        })
    }
}

/// Whether no two files share the file name they are kept under.
fn distinct_names(files: &[RemoteFile]) -> bool {
    let mut seen = HashSet::new();
    files
        .iter()
        .all(|f| seen.insert(Path::new(&f.path).file_name()))
}
