//! Researched star distances: one YAML file per star under `stars/`, for a naked-eye star that
//! Hipparcos and Gaia leave without a usable distance, in which every number is a cited value
//! read from a paper, as in a structure file.
//!
//! A star file names the star by its HIP number, lists the papers it cites, and gives the
//! distance or parallax a paper publishes for the star, with the distance's kind and the method
//! behind it; for a cluster's or association's distance given to a member, also the citation
//! that names the star a member. A star whose papers publish no distance keeps its file, saying
//! why, and places nothing.
//!
//! Each file becomes a catalogue entry of the source with no position, its identifier the bare
//! HIP number, which joins it to the Hipparcos star by the `hip` cross-identification; the entry
//! names the papers its values come from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{CatalogueEntry, Flags, PaperReference};
use univ_model::ids::Designation;
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty, Value};

use crate::normalize::{NormalizationReport, NormalizeError, Normalized};
use crate::source::{LeftOutReason, SourceDefinition};
use crate::structures::{Citation, Cited, CitedFile, Dispute, Evidence, Paper, Problem, problem};
use crate::yaml::{FileError, read_checked, read_dir, slug_of};

/// A star as its file describes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarFile {
    /// Its number in the Hipparcos catalogue.
    pub hip: u32,
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    pub papers: Vec<Paper>,
    /// The distance a paper publishes for the star.
    #[serde(default)]
    pub distance: Option<Researched>,
    /// A parallax of the star itself, measured otherwise than by Hipparcos-2.
    #[serde(default)]
    pub parallax: Option<Researched>,
    /// For a cluster's or association's distance: where a paper names the star a member.
    #[serde(default)]
    pub membership: Option<Citation>,
    /// Why no distance is given, for a star whose papers publish none.
    #[serde(default)]
    pub unplaced: Option<String>,
    /// Other determinations that disagree, each quoted from the paper that makes it.
    #[serde(default)]
    pub disputes: Vec<Dispute>,
    pub notes: String,
}

/// A cited distance or parallax, with the kind of distance it is and the method behind it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Researched {
    #[serde(flatten)]
    pub value: Cited,
    pub distance_kind: DistanceKind,
    /// The method in a phrase, such as `interferometric orbit` or `Sco-Cen subgroup distance`.
    pub method: String,
}

/// A star file that cannot be used, naming the file, the field and what is wrong.
pub type StarError = FileError;

/// A star file's star, or why it cannot be used.
type StarRead = Result<StarFile, StarError>;

impl CitedFile for StarFile {
    fn papers(&self) -> &[Paper] {
        &self.papers
    }
}

impl StarFile {
    /// Reads and checks the star in `file`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse, on a number that cites
    /// nothing or that its quoted sentence does not hold, and on the checks of
    /// [`StarFile::check`].
    pub fn read(file: &Path) -> Result<Self, StarError> {
        read_checked(file, Self::check)
    }

    /// The checks serde cannot make: the papers stand, every citation names one of them and
    /// holds its number, the star has one positive distance or parallax or says why not, only
    /// a group's distance cites the star's membership, and every number of the free text is
    /// cited.
    fn check(&self) -> Result<(), Problem> {
        if self.hip == 0 {
            return problem("hip", "is not a Hipparcos number");
        }
        if self.name.trim().is_empty() || self.notes.trim().is_empty() {
            return problem("name", "the name and the notes must not be empty");
        }
        self.check_papers()?;
        match (&self.distance, &self.parallax, &self.unplaced) {
            (None, None, None) => {
                return problem("unplaced", "give a distance, a parallax or why not");
            }
            (Some(_), Some(_), _) => {
                return problem("parallax", "give a distance or a parallax, not both");
            }
            (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
                return problem("unplaced", "is given for a star with a distance");
            }
            (None, None, Some(why)) if why.trim().is_empty() => {
                return problem("unplaced", "is empty");
            }
            _ => {}
        }
        for (field, r) in self.researched() {
            if r.method.trim().is_empty() {
                return problem(&format!("{field}.method"), "is empty");
            }
            if r.value.value() <= 0.0 {
                return problem(field, "is not positive");
            }
            self.check_citation(field, &r.value.cite, Some(&r.value))?;
            self.check_value(field, &r.value)?;
        }
        if let Some(p) = &self.parallax {
            if p.value.unit() != Unit::Milliarcsecond {
                return problem("parallax.unit", "a parallax is given in milliarcseconds");
            }
            if p.distance_kind != DistanceKind::Parallax || p.value.kind == KindOfValue::Inferred {
                return problem(
                    "parallax",
                    "a parallax of the star itself is of kind parallax, measured or, resting on \
                     an assumed mass, estimated",
                );
            }
            if !matches!(p.value.quantity.uncertainty, Uncertainty::Symmetric { .. }) {
                return problem(
                    "parallax.uncertainty",
                    "a parallax is inverted only with a symmetric error; give the paper's \
                     distance instead",
                );
            }
        }
        if let Some(d) = &self.distance
            && !matches!(d.value.unit(), Unit::Parsec | Unit::Kiloparsec)
        {
            return problem(
                "distance.unit",
                "a star's distance is given in parsecs or kiloparsecs",
            );
        }
        let group = self
            .distance
            .as_ref()
            .is_some_and(|d| d.value.kind == KindOfValue::Inferred);
        if group != self.membership.is_some() {
            return problem(
                "membership",
                "a group's distance given to a member is inferred and cites the membership; only \
                 it does",
            );
        }
        if let Some(m) = &self.membership {
            self.check_citation("membership", m, None)?;
        }
        self.check_disputes(&self.disputes)?;
        self.evidence().check()
    }

    /// The distance or parallax, by field.
    fn researched(&self) -> impl Iterator<Item = (&'static str, &Researched)> {
        [("distance", &self.distance), ("parallax", &self.parallax)]
            .into_iter()
            .filter_map(|(f, r)| Some((f, r.as_ref()?)))
    }

    /// The file's free text beside what its citations show: the star's names, the papers'
    /// references, every citation and the cited values.
    fn evidence(&self) -> Evidence<'_> {
        let mut evidence = Evidence::default();
        evidence.text(&self.name);
        for name in &self.alternative_names {
            evidence.text(name);
        }
        for paper in &self.papers {
            evidence.text(&paper.reference);
        }
        for cite in self
            .disputes
            .iter()
            .map(|d| &d.cite)
            .chain(&self.membership)
        {
            evidence.cite(cite);
        }
        evidence.free("notes", &self.notes);
        if let Some(why) = &self.unplaced {
            evidence.free("unplaced", why);
        }
        for (field, r) in self.researched() {
            evidence.value(field, &r.value);
            evidence.free(format!("{field}.method"), &r.method);
        }
        evidence
    }

    /// The papers the star's values come from, the paper of its distance first, each once.
    fn references(&self) -> Vec<PaperReference> {
        let cites = self
            .researched()
            .flat_map(|(_, r)| {
                std::iter::once(&r.value.cite).chain(r.value.uncertainty_cite.as_deref())
            })
            .chain(&self.membership);
        let mut ids: Vec<&str> = vec![];
        for cite in cites {
            if !ids.contains(&cite.paper.as_str()) {
                ids.push(&cite.paper);
            }
        }
        ids.into_iter()
            .filter_map(|id| self.papers.iter().find(|p| p.id == id))
            .map(|p| PaperReference {
                citation: p.reference.clone(),
                doi: p.doi.clone(),
                bibcode: p.bibcode.clone(),
            })
            .collect()
    }

    /// The star's catalogue entry in `definition`'s source, or why it has none: a star whose
    /// papers publish no distance gives no entry.
    ///
    /// # Errors
    ///
    /// Fails when the distance or parallax cannot stand as a univ-model value, naming why.
    pub fn entry(&self, definition: &SourceDefinition) -> Result<Option<CatalogueEntry>, String> {
        if self.unplaced.is_some() {
            return Ok(None);
        }
        let designation = Designation::new(definition.id.clone(), self.hip.to_string())
            .map_err(|e| e.to_string())?;
        let distances = self
            .distance
            .as_ref()
            .map(|d| {
                Distance::new(
                    d.value.quantity,
                    None,
                    Unit::Parsec,
                    d.value.kind,
                    (d.distance_kind, Method::Other(d.method.clone())),
                    Measure::Euclidean,
                    None,
                )
            })
            .transpose()
            .map_err(|e| e.to_string())?;
        let parallax = self
            .parallax
            .as_ref()
            .map(|p| Value::normalize(p.value.quantity, Unit::Degree, p.value.kind))
            .transpose()
            .map_err(|e| e.to_string())?;
        // A parallax keeps its method, as a distance does in its own.
        let flags = self
            .parallax
            .iter()
            .map(|p| ("method".to_owned(), p.method.clone()))
            .collect();
        let entry = CatalogueEntry {
            cross_ids: Flags::default(),
            designation,
            name: Some(self.name.as_str().into()),
            position: None,
            distances: distances.into_iter().collect(),
            redshifts: thin_vec::thin_vec![],
            velocity: None,
            parallax: parallax.map(Box::new),
            proper_motion: None,
            magnitude: None,
            colours: thin_vec::thin_vec![],
            spectral_type: None,
            group_id: None,
            supergalactic_velocity: None,
            flags,
            references: self.references().into(),
        };
        entry.check().map_err(|e| e.to_string())?;
        Ok(Some(entry))
    }
}

/// Each star file of `dir` in file-name order, with its star or why it cannot be used: a file
/// name that is not a kebab-case slug, a star that fails [`StarFile::read`], or a HIP number an
/// earlier file gives.
fn read_each(dir: &Path) -> Result<Vec<(PathBuf, StarRead)>, StarError> {
    let mut hips = BTreeMap::new();
    Ok(read_dir(dir)?
        .into_iter()
        .map(|file| {
            let star = slug_of(&file).and_then(|slug| {
                let star = StarFile::read(&file)?;
                match hips.insert(star.hip, slug.to_owned()) {
                    Some(other) => Err(FileError::new(
                        &file,
                        "hip",
                        format!("is also the star of {other}"),
                    )),
                    None => Ok(star),
                }
            });
            (file, star)
        })
        .collect())
}

/// Reads every star file in `dir`, in file-name order, refusing on the first that cannot be
/// used. A file is named after its star: `<kebab-slug>.yaml`.
///
/// # Errors
///
/// Fails when the directory cannot be read, a file name is not a kebab-case slug, two files
/// give one HIP number, or a star fails [`StarFile::read`].
pub fn read_all(dir: &Path) -> Result<Vec<(PathBuf, StarFile)>, StarError> {
    read_each(dir)?
        .into_iter()
        .map(|(file, star)| Ok((file, star?)))
        .collect()
}

/// Every problem of the star files in `dir`, one line each, in file-name order: each file's
/// first problem, and a HIP number two files give. Empty when every file can be used.
///
/// # Errors
///
/// Fails when the directory cannot be read.
pub fn problems(dir: &Path) -> Result<Vec<String>, StarError> {
    Ok(read_each(dir)?
        .into_iter()
        .filter_map(|(_, star)| star.err())
        .map(|e| e.to_string())
        .collect())
}

/// The researched star distances of `definition`, read from the star files in `dir`: an entry
/// for each star with a distance or parallax, and each star without one counted as left out
/// for having no distance.
///
/// # Errors
///
/// Fails on the first star file that cannot be used.
pub fn normalize(definition: &SourceDefinition, dir: &Path) -> Result<Normalized, NormalizeError> {
    let refused = |file: PathBuf, error: String| NormalizeError::Table {
        source_id: definition.id.clone(),
        file,
        error,
    };
    let stars = read_all(dir).map_err(|e| refused(dir.to_owned(), e.to_string()))?;
    let mut report = NormalizationReport::default();
    let mut entries = vec![];
    for (file, star) in stars {
        match star.entry(definition).map_err(|e| refused(file, e))? {
            Some(entry) => {
                entries.push(entry);
                report.published += 1;
            }
            None => {
                *report
                    .left_out
                    .entry(LeftOutReason::NoPublishedDistance)
                    .or_default() += 1;
            }
        }
    }
    Ok(Normalized {
        source: definition.id.clone(),
        entries,
        report,
    })
}
