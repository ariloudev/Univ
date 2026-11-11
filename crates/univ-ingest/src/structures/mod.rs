//! Researched structures: one YAML file per structure under `structures/`, in which every number
//! is a cited value read from a paper (RQ-20 to RQ-23).
//!
//! A structure file names the structure, its kind and status, its scale domain, the papers it
//! cites, its published position and geometry in a named frame, its other published values, the
//! disputes about it, and, for a structure outlined from a volume grid, the grid, threshold and
//! test.
//!
//! # Scale domain
//!
//! `scale_domain` is the scale domain a structure is listed and shown in, one of those the
//! source definitions give their sources. It is a classification, not a published value, set
//! from the structure's published distance against the boundaries the definitions' domains
//! imply:
//!
//! - `stars`: within the Milky Way, which the stellar catalogues (Hipparcos, Gaia, the GCNS)
//!   cover;
//! - `nearby_galaxies`: beyond it, out to 11 Mpc, the depth of the deepest `nearby_galaxies`
//!   source, the Updated Nearby Galaxy Catalog (Karachentsev et al. 2013);
//! - `cosmic_web`: beyond 11 Mpc, which the redshift surveys, Cosmicflows and the volume grids
//!   cover.
//!
//! The published distance is that of the structure's published position or the centre of its
//! geometry; for one about the observer, its published radius; for one whose papers give no
//! position, the distance or redshift they give it. Every structure of `structures/` but the
//! Galaxy's spiral arms lies beyond 11 Mpc (the nearest, the Local Void's deepest minimum, at
//! about 30 Mpc) and is `cosmic_web`. The spiral arms, computed from a model of the arms'
//! masers, are listed with the Galaxy tracers they are drawn among: `galaxy_tracers`.

mod check;
mod citation;
mod cited;
mod geometry;
mod members;
mod numbers;
mod outline;
mod shape;
mod spiral;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::source::ScaleDomain;
use crate::yaml::{FileError, read_checked, read_dir, slug_of};

pub(crate) use check::{CitedFile, Evidence, Problem, problem};
pub use citation::{Citation, Locator};
pub use cited::{Assumes, Cited};
pub use geometry::{AxisExtents, Coordinates, Radial};
pub use members::{Column, ColumnName, Format, MemberSource, Members, Row};
pub use outline::{
    Choice, ChosenBy, DensePlaces, GridOutline, OutlineTest, Rule, Seed, Side, Threshold,
};
pub use shape::{AxisBox, Sides, SkyBox, SphereSize, StructureGeometry, StructurePrimitive};
pub use spiral::{EXTRAPOLATION_DEGREES, Extent, LogSpiral, RADIUS_BAND_KPC};

/// A researched structure as its file describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    pub kind: StructureKind,
    pub status: Status,
    /// The scale domain it is listed in, by its published distance (see the module's docs).
    pub scale_domain: ScaleDomain,
    pub papers: Vec<Paper>,
    /// The published point position: the fallback when an outline fails its test.
    #[serde(default)]
    pub position: Option<Coordinates>,
    #[serde(default)]
    pub geometry: Option<StructureGeometry>,
    /// Its published extents along the axes of its position's frame, where its papers do not
    /// say where it ends; it is placed at its position, with these about it.
    #[serde(default)]
    pub extents: Option<AxisExtents>,
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
    /// Where a reconstructed peculiar-velocity flow diverges from: an underdense region that
    /// pushes matter away.
    Repeller,
    /// Where a reconstructed peculiar-velocity flow converges: an overdense region that matter
    /// streams toward, such as the Great Attractor.
    Attractor,
    /// A claimed concentration of tracers, such as gamma-ray bursts, at one redshift.
    Overdensity,
    /// A spiral arm of the Galaxy over the range of azimuth its published fit covers.
    SpiralArm,
    /// A spiral arm's fitted formula carried on beyond the range its fit covers.
    SpiralArmExtrapolation,
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
    /// Computed from a published model, such as a log-spiral fit to an arm's tracers.
    Model,
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
pub type StructureError = FileError;

impl Structure {
    /// Reads and checks the structure in `file`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse, on a number that cites
    /// nothing (naming the number), on a value read from a figure that is not estimated, and on
    /// the checks of [`Structure::check`].
    pub fn read(file: &Path) -> Result<Self, StructureError> {
        read_checked(file, Self::check)
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
        let ids = self.check_papers()?;
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
            self.check_value(&field, value)?;
        }
        if let Some(StructureGeometry::SkyRegion(b)) = &self.geometry
            && let Some(at_least) = &b.at_least
        {
            self.check_citation("geometry.sky_region.at_least", at_least, None)?;
        }
        self.check_disputes(&self.disputes)?;
        if let Some(StructureGeometry::Members(m)) = &self.geometry
            && !ids.contains(m.source.paper())
        {
            return problem("geometry.members.source.paper", "is not among the papers");
        }
        self.check_members()?;
        self.check_extents()?;
        self.check_spiral()?;
        self.check_free_text()
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
            if let Threshold::Cited(c) = &outline.threshold {
                out.push(("outline.threshold".to_owned(), c.as_ref()));
            }
            if let Some(seed) = &outline.seed {
                for (field, c) in seed.position.cited() {
                    out.push((format!("outline.seed.position.{field}"), c));
                }
            }
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
    Ok(read_all_named(dir)?.into_iter().map(|(_, s)| s).collect())
}

/// [`read_all`], each structure with its id: the slug its file is named by.
///
/// # Errors
///
/// As [`read_all`].
pub fn read_all_named(dir: &Path) -> Result<Vec<(String, Structure)>, StructureError> {
    read_dir(dir)?
        .iter()
        .map(|file| Ok((slug_of(file)?.to_owned(), Structure::read(file)?)))
        .collect()
}

impl Structure {
    /// Checks a structure already read as its loader does: every number cites one of its
    /// papers, a quoted sentence holds its number, and it is placed or says why not.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem of the first check that fails.
    pub fn check_cited(&self) -> Result<(), (String, String)> {
        self.check()
    }
}
