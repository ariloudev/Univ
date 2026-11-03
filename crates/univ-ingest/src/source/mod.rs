//! Source definitions: one YAML file per source under `sources/`, read and checked before
//! anything is fetched.

mod normalization;
mod reference;
mod rules;

pub use reference::{Reference, is_bibcode, is_doi};

pub use normalization::{
    Coded, ComovingVelocity, DistanceReading, ErrorColumns, MethodCodes, Normalization, Published,
    RedshiftCodes, RedshiftReading,
};
pub use rules::{
    Condition, DistanceOverride, FixAction, KindOverride, LeftOutReason, MAX_FIXES, Rule,
};

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use univ_model::ids::SourceId;
use univ_model::position::{Epoch, Frame};
use univ_model::units::Unit;

use crate::yaml::{FileError, read_checked, read_dir};

/// A source as its definition in the repository describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub id: SourceId,
    pub title: String,
    /// The provider's id, such as `vizier` or `gaia-archive`.
    pub provider: String,
    pub access: Access,
    /// The selection the query makes, each in words or in the provider's syntax.
    pub cuts: Vec<String>,
    pub columns: Vec<Column>,
    /// The frame and epoch of the source's positions.
    pub frame: Frame,
    pub epoch: Epoch,
    pub citation: String,
    /// The primary reference, the paper `citation` names first, by its bibcode and DOI.
    pub reference: Reference,
    pub terms: Terms,
    pub redistribution: Redistribution,
    pub scale_domain: ScaleDomain,
    /// The base URLs to try, in order; none for a source kept in this repository.
    pub mirrors: Vec<String>,
    /// Corrections to the source as published, each with its evidence.
    #[serde(default)]
    pub fixes: Vec<Fix>,
    /// How rows are read beyond the column each field comes from. Its enums are written as
    /// single-key maps, such as `{ symmetric: e_cz }`, rather than YAML tags.
    #[serde(default, with = "serde_norway::with::singleton_map_recursive")]
    pub normalization: Normalization,
}

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
    /// ESA Gaia archive stops anonymous answers at 50,000 rows without saying so in CSV.
    Tap {
        query: String,
        chunks: u32,
        #[serde(default)]
        count: Option<String>,
    },
    /// Files under each mirror, each fetched whole and resumable.
    Files { files: Vec<RemoteFile> },
    /// Files kept in this repository under `path`, such as the researched structures, which are
    /// read from their papers rather than fetched.
    Repository { path: String },
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

/// A column the source definition takes, mapped to a univ-model field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    /// The column's name as the provider gives it.
    pub column: String,
    pub field: Field,
    /// The column's unit as published.
    pub unit: Unit,
}

/// The univ-model field a column fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Identifier,
    Name,
    RightAscension,
    Declination,
    RightAscensionError,
    DeclinationError,
    Redshift,
    RedshiftError,
    Velocity,
    VelocityError,
    Distance,
    DistanceLow,
    DistanceHigh,
    DistanceError,
    DistanceModulus,
    DistanceModulusError,
    Parallax,
    ParallaxError,
    ProperMotionRa,
    ProperMotionDec,
    Magnitude,
    GroupId,
    Quality,
    SupergalacticX,
    SupergalacticY,
    SupergalacticZ,
    PhotometricRedshift,
    PhotometricRedshiftError,
    /// A density contrast δ, or its standard deviation, on a grid.
    Density,
    DensityError,
    /// A radial peculiar velocity, or its standard deviation, on a grid.
    RadialVelocity,
    RadialVelocityError,
    /// A three-component peculiar velocity, or its standard deviation, on a grid.
    VelocityVector,
    VelocityVectorError,
    /// The label of the basin of attraction a grid cell belongs to.
    BasinLabel,
}

/// The terms of use a source is published under.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terms {
    pub url: String,
    /// The terms, quoted as published.
    pub quote: String,
    /// The credit the source requires, quoted as published.
    pub credit: String,
}

/// Whether a source's data may be published in a release, judged from its terms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Redistribution {
    pub status: RedistributionStatus,
    /// Whether the judgement is provisional or settled. Every definition states it.
    pub review: Review,
    pub reasoning: String,
}

/// How far the redistribution judgement has been reviewed. A local build reports the sources
/// whose judgement is provisional and builds with them; refusing them is the public
/// deployment's gate, not a local build's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    /// Judged from what could be read so far, to be judged again before any public deployment.
    Provisional,
    /// Judged again and settled.
    Settled,
}

impl Review {
    /// The review as a definition writes it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Settled => "settled",
        }
    }
}

impl RedistributionStatus {
    /// The status as a definition writes it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permitted => "permitted",
            Self::PermittedWithCredit => "permitted_with_credit",
            Self::Forbidden => "forbidden",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionStatus {
    Permitted,
    PermittedWithCredit,
    Forbidden,
}

/// The range of scales a source belongs to, each with its own octree and unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleDomain {
    Stars,
    NearbyGalaxies,
    CosmicWeb,
}

/// A correction to a source as published.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fix {
    pub defect: String,
    pub change: String,
    pub evidence: String,
    /// The change in the form the normalizer applies; none when the definition's columns
    /// already express it.
    #[serde(default)]
    pub action: Option<FixAction>,
}

/// A source definition that cannot be used, naming its file, the field and what is wrong.
pub type DefinitionError = FileError;

/// A release refused because it holds a source whose terms forbid redistribution.
#[derive(Debug, Error, PartialEq)]
#[error(
    "source {source_id} forbids redistribution, so no release may hold it. Its terms ({url}) say: \"{quote}\""
)]
pub struct ForbiddenSource {
    pub source_id: SourceId,
    pub url: String,
    pub quote: String,
}

impl SourceDefinition {
    /// Reads and checks the definition in `file`, whose stem must be the source's id.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse or a definition that is
    /// incomplete, names an unknown unit, frame or scale domain, or breaks a check below.
    pub fn read(file: &Path) -> Result<Self, DefinitionError> {
        read_checked(file, |definition: &Self| {
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if definition.id.as_str() != stem {
                return Err((
                    "id".to_owned(),
                    format!("is {} but the file is named {stem}", definition.id),
                ));
            }
            definition.check()
        })
    }

    /// The checks serde cannot make, as the field and the problem.
    fn check(&self) -> Result<(), (String, String)> {
        let blank = |field: &str, value: &str| {
            if value.trim().is_empty() {
                Err((field.to_owned(), "is empty".to_owned()))
            } else {
                Ok(())
            }
        };
        blank("title", &self.title)?;
        blank("provider", &self.provider)?;
        blank("citation", &self.citation)?;
        self.reference.check()?;
        blank("terms.url", &self.terms.url)?;
        blank("terms.quote", &self.terms.quote)?;
        blank("terms.credit", &self.terms.credit)?;
        blank("redistribution.reasoning", &self.redistribution.reasoning)?;
        self.access.check()?;
        if self.columns.is_empty() && !matches!(self.access, Access::Repository { .. }) {
            return Err(("columns".to_owned(), "lists no column".to_owned()));
        }
        let mut seen = HashSet::new();
        for (i, column) in self.columns.iter().enumerate() {
            blank(&format!("columns[{i}].column"), &column.column)?;
            if !seen.insert(&column.column) {
                return Err((
                    format!("columns[{i}].column"),
                    format!("repeats {}", column.column),
                ));
            }
        }
        if !(1_900.0..=2_100.0).contains(&self.epoch.0) {
            return Err((
                "epoch".to_owned(),
                format!("{} is not a Julian year", self.epoch.0),
            ));
        }
        if self.mirrors.is_empty() && !matches!(self.access, Access::Repository { .. }) {
            return Err(("mirrors".to_owned(), "lists no mirror".to_owned()));
        }
        for (i, mirror) in self.mirrors.iter().enumerate() {
            if !(mirror.starts_with("https://") || mirror.starts_with("http://")) {
                return Err((
                    format!("mirrors[{i}]"),
                    format!("{mirror} is not an http(s) URL"),
                ));
            }
        }
        for (i, fix) in self.fixes.iter().enumerate() {
            blank(&format!("fixes[{i}].defect"), &fix.defect)?;
            blank(&format!("fixes[{i}].change"), &fix.change)?;
            blank(&format!("fixes[{i}].evidence"), &fix.evidence)?;
        }
        normalization::check(&self.normalization, &self.columns, &self.fixes)
    }

    /// Refuses a source whose terms forbid redistribution, quoting the terms.
    ///
    /// # Errors
    ///
    /// Fails when the redistribution status is forbidden.
    pub fn publishable(&self) -> Result<(), ForbiddenSource> {
        if self.redistribution.status == RedistributionStatus::Forbidden {
            return Err(ForbiddenSource {
                source_id: self.id.clone(),
                url: self.terms.url.clone(),
                quote: self.terms.quote.clone(),
            });
        }
        Ok(())
    }
}

impl Access {
    fn check(&self) -> Result<(), (String, String)> {
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
            Self::Repository { path } if path.trim().is_empty() => {
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

/// Reads every definition in `dir`, in file-name order, refusing the run on the first that
/// cannot be used.
///
/// # Errors
///
/// Fails when the directory cannot be read or a definition fails [`SourceDefinition::read`].
pub fn read_all(dir: &Path) -> Result<Vec<SourceDefinition>, DefinitionError> {
    read_dir(dir)?
        .iter()
        .map(|file| SourceDefinition::read(file))
        .collect()
}
