//! Researched structures: one YAML file per structure under `structures/`, in which every number
//! is a cited value read from a paper (RQ-20 to RQ-23).
//!
//! A structure file names the structure, its kind and status, the papers it cites, its published
//! position and geometry in a named frame, its other published values, the disputes about it,
//! and, for a structure outlined from a volume grid, the grid, threshold and test.

mod cited;
mod geometry;
mod members;
mod outline;
mod shape;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use cited::{Citation, Cited, Locator};
pub use geometry::{Coordinates, Radial, StructureFrame};
pub use members::{Column, ColumnName, Format, MemberSource, Members, Row};
pub use outline::{GridOutline, OutlineTest, Side};
pub use shape::{SkyBox, SphereSize, StructureGeometry, StructurePrimitive};

/// A researched structure as its file describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    pub kind: StructureKind,
    pub status: Status,
    pub papers: Vec<Paper>,
    /// The published point position: the fallback when an outline fails its test.
    #[serde(default)]
    pub position: Option<Coordinates>,
    #[serde(default)]
    pub geometry: Option<StructureGeometry>,
    /// Why a structure has neither position nor geometry, when its papers give neither.
    #[serde(default)]
    pub unplaced: Option<String>,
    /// Other published values, such as extents, depths and counts, by name.
    #[serde(default)]
    pub values: BTreeMap<String, Cited>,
    #[serde(default)]
    pub disputes: Vec<Dispute>,
    #[serde(default)]
    pub outline: Option<GridOutline>,
    pub notes: String,
}

/// What sort of structure it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureKind {
    Void,
    Wall,
    Filament,
    Supercluster,
    SuperclusterComplex,
    Basin,
    QuasarGroup,
    Arc,
    Ring,
    Shell,
    Repeller,
    /// A claimed concentration of tracers, such as gamma-ray bursts, at one redshift.
    Overdensity,
}

/// How the structure is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Seen in the positions and redshifts of a survey or catalogue.
    Observed,
    /// Reconstructed by a model, such as a Wiener filter of peculiar velocities.
    Reconstructed,
    /// Claimed from the clustering statistics of sparse tracers; its significance is debated.
    StatisticalClaim,
}

/// A paper a structure file cites.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paper {
    /// The id the file's citations use, such as `tully-2019`.
    pub id: String,
    /// Authors, year, journal, volume and page.
    pub reference: String,
    pub title: String,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub bibcode: Option<String>,
    #[serde(default)]
    pub arxiv: Option<String>,
    /// How the paper was read: the full text, the arXiv source or the abstract only.
    pub read: String,
}

/// A published dispute of the structure, quoted from the paper that makes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dispute {
    pub claim: String,
    pub cite: Citation,
}

/// A structure file that cannot be used, naming the file, the field and what is wrong.
#[derive(Debug, Error, PartialEq)]
#[error("{}: {field}: {problem}", file.display())]
pub struct StructureError {
    pub file: PathBuf,
    pub field: String,
    pub problem: String,
}

impl Structure {
    /// Reads and checks the structure in `file`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse, on a number that cites
    /// nothing (naming the number), on a value read from a figure that is not estimated, and on
    /// the checks of [`Structure::check`].
    pub fn read(file: &Path) -> Result<Self, StructureError> {
        let fail = |field: &str, problem: String| StructureError {
            file: file.to_path_buf(),
            field: field.to_owned(),
            problem,
        };
        let text = fs::read_to_string(file).map_err(|e| fail("(file)", e.to_string()))?;
        let document = serde_norway::Deserializer::from_str(&text);
        let structure: Self = serde_path_to_error::deserialize(document).map_err(|e| {
            let path = e.path().to_string();
            let message = e.into_inner().to_string();
            let problem = message
                .strip_prefix(&format!("{path}: "))
                .unwrap_or(&message);
            let field = if path == "." { "(top level)" } else { &path };
            fail(field, problem.to_owned())
        })?;
        structure
            .check()
            .map_err(|(field, problem)| fail(&field, problem))?;
        Ok(structure)
    }

    /// The checks serde cannot make, as the field and the problem: every citation names one of
    /// the file's papers, every quoted sentence holds its number, and the structure is placed or
    /// says why not.
    fn check(&self) -> Result<(), (String, String)> {
        let problem = |field: &str, what: &str| Err((field.to_owned(), what.to_owned()));
        if self.name.trim().is_empty() {
            return problem("name", "is empty");
        }
        if self.notes.trim().is_empty() {
            return problem("notes", "is empty");
        }
        let mut ids = HashSet::new();
        for (i, paper) in self.papers.iter().enumerate() {
            let slug = !paper.id.is_empty()
                && paper
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !slug || !ids.insert(paper.id.as_str()) {
                return problem(&format!("papers[{i}].id"), "is not a unique lowercase slug");
            }
            if paper.doi.is_none() && paper.bibcode.is_none() {
                return problem(&format!("papers[{i}]"), "needs a DOI or a bibcode");
            }
            if paper.reference.trim().is_empty() || paper.read.trim().is_empty() {
                return problem(
                    &format!("papers[{i}]"),
                    "needs its reference and how it was read",
                );
            }
        }
        let placed = self.position.is_some() || self.geometry.is_some();
        match (placed, &self.unplaced) {
            (false, None) => return problem("unplaced", "give a position, a geometry or why not"),
            (true, Some(_)) => return problem("unplaced", "is given for a placed structure"),
            (false, Some(why)) if why.trim().is_empty() => return problem("unplaced", "is empty"),
            _ => {}
        }
        if self.outline.is_some() && self.position.is_none() {
            return problem(
                "position",
                "a structure outlined from a grid needs its published position, where it is \
                 shown when the outline fails its test",
            );
        }
        for (field, value) in self.cited_values() {
            self.check_citation(&field, &value.cite, Some(value))?;
        }
        for (i, dispute) in self.disputes.iter().enumerate() {
            let field = format!("disputes[{i}]");
            if dispute.claim.trim().is_empty() {
                return problem(&field, "states no claim");
            }
            self.check_citation(&format!("{field}.cite"), &dispute.cite, None)?;
        }
        if let Some(StructureGeometry::Members(m)) = &self.geometry
            && !ids.contains(m.source.paper())
        {
            return problem("geometry.members.source.paper", "is not among the papers");
        }
        Ok(())
    }

    fn check_citation(
        &self,
        field: &str,
        cite: &Citation,
        value: Option<&Cited>,
    ) -> Result<(), (String, String)> {
        if !self.papers.iter().any(|p| p.id == cite.paper) {
            return Err((
                format!("{field}.cite.paper"),
                format!("{} is not among the papers", cite.paper),
            ));
        }
        if let (Some(value), Locator::Sentence { quote, .. }) = (value, &cite.locator)
            && !value.appears_in(quote)
        {
            let shown = value
                .as_published
                .clone()
                .unwrap_or_else(|| value.value().to_string());
            return Err((
                field.to_owned(),
                format!("the number {shown} does not appear in the sentence it cites"),
            ));
        }
        Ok(())
    }

    /// Every cited number in the file with its field path, except the cells of a member table,
    /// which cite the table through its source.
    #[must_use]
    pub fn cited_values(&self) -> Vec<(String, &Cited)> {
        let mut out = vec![];
        if let Some(position) = &self.position {
            for (field, c) in position.cited() {
                out.push((format!("position.{field}"), c));
            }
        }
        if let Some(geometry) = &self.geometry {
            for (field, c) in geometry.cited() {
                out.push((format!("geometry.{field}"), c));
            }
        }
        for (name, c) in &self.values {
            out.push((format!("values.{name}"), c));
        }
        if let Some(outline) = &self.outline {
            out.push(("outline.threshold".to_owned(), &outline.threshold));
        }
        out
    }

    /// The univ-model geometry kind: the geometry's, or `point` for a structure with only a
    /// position.
    #[must_use]
    pub fn geometry_kind(&self) -> Option<&'static str> {
        match (&self.geometry, &self.position) {
            (Some(g), _) => Some(g.kind()),
            (None, Some(_)) => Some("point"),
            (None, None) => None,
        }
    }
}

/// Reads every structure file in `dir`, in file-name order, refusing on the first that cannot be
/// used. A file is named after its structure: `<kebab-slug>.yaml`.
///
/// # Errors
///
/// Fails when the directory cannot be read, a file name is not a kebab-case slug, or a structure
/// fails [`Structure::read`].
pub fn read_all(dir: &Path) -> Result<Vec<Structure>, StructureError> {
    let fail = |file: &Path, field: &str, problem: String| StructureError {
        file: file.to_path_buf(),
        field: field.to_owned(),
        problem,
    };
    let entries = fs::read_dir(dir).map_err(|e| fail(dir, "(directory)", e.to_string()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|file| {
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let kebab = stem.starts_with(|c: char| c.is_ascii_lowercase())
                && !stem.contains("--")
                && !stem.ends_with('-')
                && stem
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !kebab {
                return Err(fail(file, "(file name)", "is not a kebab-case slug".into()));
            }
            Structure::read(file)
        })
        .collect()
}
