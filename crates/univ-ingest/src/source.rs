//! Source definitions: one YAML file per source under `sources/`, read and checked before
//! anything is fetched.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use univ_model::ids::SourceId;
use univ_model::position::{Epoch, Frame};
use univ_model::units::Unit;

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
    pub terms: Terms,
    pub redistribution: Redistribution,
    pub scale_domain: ScaleDomain,
    /// The base URLs to try, in order.
    pub mirrors: Vec<String>,
    /// Corrections to the source as published, each with its evidence.
    #[serde(default)]
    pub fixes: Vec<Fix>,
}

/// How a source is fetched from each mirror.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "method", deny_unknown_fields)]
pub enum Access {
    /// A `VizieR` table through `asu-tsv`, with these constraints, in `chunks` ranges of declination.
    VizierTsv {
        table: String,
        constraints: Vec<String>,
        chunks: u32,
    },
    /// An SQL query to `SkyServer`, split into `chunks` by `{chunk}` and `{chunks}` in the query.
    SkyserverSql { query: String, chunks: u32 },
    /// An ADQL query to a TAP service, split into `chunks` by `{chunk}` and `{chunks}` in the query.
    Tap { query: String, chunks: u32 },
    /// A file at `path` under each mirror, with the size and SHA-256 its provider publishes.
    File {
        path: String,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        sha256: Option<String>,
    },
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
    pub reasoning: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionStatus {
    Permitted,
    PermittedWithCredit,
    Forbidden,
}

/// The range of scales a source belongs to, each with its own octree and unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

/// A source definition that cannot be used, naming its file, the field and what is wrong.
#[derive(Debug, Error, PartialEq)]
#[error("{}: {field}: {problem}", file.display())]
pub struct DefinitionError {
    pub file: PathBuf,
    pub field: String,
    pub problem: String,
}

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
        let fail = |field: &str, problem: String| DefinitionError {
            file: file.to_path_buf(),
            field: field.to_owned(),
            problem,
        };
        let text = fs::read_to_string(file).map_err(|e| fail("(file)", e.to_string()))?;
        let document = serde_norway::Deserializer::from_str(&text);
        let definition: Self = serde_path_to_error::deserialize(document).map_err(|e| {
            let path = e.path().to_string();
            // The YAML error may already lead with the field; say it once.
            let message = e.into_inner().to_string();
            let problem = message
                .strip_prefix(&format!("{path}: "))
                .unwrap_or(&message);
            let field = if path == "." { "(top level)" } else { &path };
            fail(field, problem.to_owned())
        })?;
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if definition.id.as_str() != stem {
            return Err(fail(
                "id",
                format!("is {} but the file is named {stem}", definition.id),
            ));
        }
        definition
            .check()
            .map_err(|(field, problem)| fail(&field, problem))?;
        Ok(definition)
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
        blank("terms.url", &self.terms.url)?;
        blank("terms.quote", &self.terms.quote)?;
        blank("terms.credit", &self.terms.credit)?;
        blank("redistribution.reasoning", &self.redistribution.reasoning)?;
        self.access.check()?;
        if self.columns.is_empty() {
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
        if self.mirrors.is_empty() {
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
        Ok(())
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
            Self::VizierTsv { chunks: 0, .. }
            | Self::SkyserverSql { chunks: 0, .. }
            | Self::Tap { chunks: 0, .. } => Some(("access.chunks", "is zero")),
            Self::File { path, .. } if path.trim().is_empty() => Some(("access.path", "is empty")),
            Self::File {
                sha256: Some(digest),
                ..
            } if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) => {
                Some(("access.sha256", "is not 64 hexadecimal digits"))
            }
            _ => None,
        };
        problem.map_or(Ok(()), |(field, what)| {
            Err((field.to_owned(), what.to_owned()))
        })
    }
}

/// Reads every definition in `dir`, in file-name order, refusing the run on the first that
/// cannot be used.
///
/// # Errors
///
/// Fails when the directory cannot be read or a definition fails [`SourceDefinition::read`].
pub fn read_all(dir: &Path) -> Result<Vec<SourceDefinition>, DefinitionError> {
    let entries = fs::read_dir(dir).map_err(|e| DefinitionError {
        file: dir.to_path_buf(),
        field: "(directory)".to_owned(),
        problem: e.to_string(),
    })?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|file| SourceDefinition::read(file))
        .collect()
}
