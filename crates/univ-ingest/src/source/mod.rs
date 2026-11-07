//! Source definitions: one YAML file per source under `sources/`, read and checked before
//! anything is fetched.

mod access;
mod normalization;
mod reading;
mod reference;
mod rules;

pub use access::{Access, Holds, PreparedFile, RemoteFile, Slices, TapRun};
pub use reading::{DesignationForm, FlagMeanings, IDENTIFIER};
pub use reference::{Reference, is_bibcode, is_doi};

pub use normalization::{
    CatalogueError, Coded, ColourReading, ComovingVelocity, DistanceReading, ErrorColumns,
    MethodCodes, Normalization, Published, RedshiftCodes, RedshiftReading, SpectralTypeReading,
};
pub use rules::{
    Condition, DistanceOverride, FixAction, KindOverride, LeftOutReason, MAX_FIXES, Rule,
};
pub use univ_model::api::sources::Permission;
use univ_model::time::is_calendar_date;

use std::collections::{BTreeMap, HashSet};
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
    /// The form the catalogue writes a designation in, cited; none where it is written bare.
    #[serde(default)]
    pub designation: Option<DesignationForm>,
    /// What each value of a flag column means, by column, cited from the catalogue's
    /// documentation; a flag column it does not name is served with its values alone.
    #[serde(default)]
    pub flags: BTreeMap<String, FlagMeanings>,
    /// How rows are read beyond the column each field comes from. Its enums are written as
    /// single-key maps, such as `{ symmetric: e_cz }`, rather than YAML tags.
    #[serde(default, with = "serde_norway::with::singleton_map_recursive")]
    pub normalization: Normalization,
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
    /// A colour index, or its error, as published; read as `normalization.colours` names it.
    ColourIndex,
    ColourIndexError,
    /// A magnitude read only to derive a colour index from, as `normalization.colours` names it.
    ColourMagnitude,
    /// A spectral type as published; read as `normalization.spectral_type` names it.
    SpectralType,
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
    /// The clause of the terms that allows only non-commercial use, quoted as published; none
    /// when the terms allow commercial use.
    #[serde(default)]
    pub non_commercial: Option<String>,
    /// The clause of the terms that requires data adapted from the source to be shared under
    /// the same licence, quoted as published; none when the terms require no such thing.
    #[serde(default)]
    pub share_alike: Option<String>,
}

/// Whether a source's data may be published in a release, judged from its terms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Redistribution {
    pub status: RedistributionStatus,
    /// Whether the judgement is provisional or settled. Every definition states it.
    pub review: Review,
    pub reasoning: String,
    /// Whom permission to redistribute was asked of, and when: given exactly when the status
    /// is permission pending.
    #[serde(default)]
    pub permission: Option<Permission>,
}

impl Redistribution {
    /// A pending status names its request, and no other status names one.
    fn check_permission(&self) -> Result<(), (String, String)> {
        let pending = self.status == RedistributionStatus::PermissionPending;
        match &self.permission {
            None if pending => Err((
                "redistribution.permission".to_owned(),
                "is missing: a pending status names whom permission is asked of".to_owned(),
            )),
            Some(_) if !pending => Err((
                "redistribution.permission".to_owned(),
                format!("is given for the status {}", self.status.as_str()),
            )),
            Some(request) => check_request(request),
            None => Ok(()),
        }
    }
}

/// Checks a request for permission: someone named, and a calendar date once it is sent.
fn check_request(request: &Permission) -> Result<(), (String, String)> {
    if request.asked_of.trim().is_empty() {
        return Err((
            "redistribution.permission.asked_of".to_owned(),
            "is empty".to_owned(),
        ));
    }
    if let Some(date) = request.asked_on.as_deref()
        && !is_calendar_date(date)
    {
        return Err((
            "redistribution.permission.asked_on".to_owned(),
            format!("{date} is not a date as YYYY-MM-DD"),
        ));
    }
    Ok(())
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
            Self::PermissionPending => "permission_pending",
            Self::Forbidden => "forbidden",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionStatus {
    Permitted,
    PermittedWithCredit,
    /// The terms state no grant, and permission has been asked of the provider. Built and
    /// published like a permitted source, with the request served beside the terms.
    PermissionPending,
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
        if let Some(clause) = &self.terms.non_commercial {
            blank("terms.non_commercial", clause)?;
        }
        if let Some(clause) = &self.terms.share_alike {
            blank("terms.share_alike", clause)?;
        }
        reading::check(self.designation.as_ref(), &self.flags, &self.columns)?;
        self.redistribution.check_permission()?;
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
