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
use univ_model::entry::{CatalogueEntry, PaperReference};
use univ_model::ids::{Designation, is_slug};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Value};

use crate::normalize::{NormalizationReport, NormalizeError, Normalized};
use crate::source::{LeftOutReason, SourceDefinition};
use crate::structures::{
    Citation, Cited, CitedFile, Dispute, Paper, Problem, shown, uncited_in_text,
};
use crate::yaml::{FileError, read_checked, read_dir};

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
#[serde(try_from = "RawResearched")]
pub struct Researched {
    pub value: Cited,
    pub distance_kind: DistanceKind,
    /// The method in a phrase, such as `interferometric orbit` or `Sco-Cen subgroup distance`.
    pub method: String,
}

/// A researched value as written: the cited value's fields beside its kind and method.
#[derive(Deserialize)]
struct RawResearched {
    distance_kind: DistanceKind,
    method: String,
    #[serde(flatten)]
    value: BTreeMap<String, serde_norway::Value>,
}

impl TryFrom<RawResearched> for Researched {
    type Error = String;
    fn try_from(raw: RawResearched) -> Result<Self, String> {
        let mapping = raw
            .value
            .into_iter()
            .map(|(k, v)| (serde_norway::Value::String(k), v))
            .collect();
        let value =
            Cited::deserialize(serde_norway::Value::Mapping(mapping)).map_err(|e| e.to_string())?;
        Ok(Self {
            value,
            distance_kind: raw.distance_kind,
            method: raw.method,
        })
    }
}

/// A star file that cannot be used, naming the file, the field and what is wrong.
pub type StarError = FileError;

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
    /// holds its number, the star has one distance or parallax or says why not, a distance of
    /// a group cites the star's membership, and every number of the free text is cited.
    fn check(&self) -> Result<(), Problem> {
        let problem = |field: &str, what: &str| Err((field.to_owned(), what.to_owned()));
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
            self.check_citation(field, &r.value.cite, Some(&r.value))?;
            self.check_value(field, &r.value)?;
        }
        if let Some(p) = &self.parallax {
            if p.value.unit() != Unit::Milliarcsecond {
                return problem("parallax.unit", "a parallax is given in milliarcseconds");
            }
            if p.value.kind != KindOfValue::Measured || p.distance_kind != DistanceKind::Parallax {
                return problem(
                    "parallax",
                    "a parallax of the star itself is measured, of kind parallax",
                );
            }
        }
        if let Some(d) = &self.distance {
            if !matches!(d.value.unit(), Unit::Parsec | Unit::Kiloparsec) {
                return problem(
                    "distance.unit",
                    "a star's distance is given in parsecs or kiloparsecs",
                );
            }
            let group = d.value.kind == KindOfValue::Inferred;
            if group != self.membership.is_some() {
                return problem(
                    "membership",
                    "a group's distance given to a member is inferred and cites the membership; \
                     only it does",
                );
            }
        }
        if let Some(m) = &self.membership {
            self.check_citation("membership", m, None)?;
        }
        let mut free: Vec<(String, &str)> = vec![("notes".to_owned(), self.notes.as_str())];
        free.extend(self.unplaced.as_deref().map(|w| ("unplaced".to_owned(), w)));
        for (i, d) in self.disputes.iter().enumerate() {
            let field = format!("disputes[{i}]");
            if d.claim.trim().is_empty() {
                return problem(&field, "states no claim");
            }
            self.check_citation(&format!("{field}.cite"), &d.cite, None)?;
            // A dispute's numbers are those of its own citation.
            uncited_in_text(
                &[(format!("{field}.claim"), &d.claim)],
                &shown(&d.cite),
                &[],
            )?;
        }
        for (field, r) in self.researched() {
            free.extend(
                r.value
                    .note
                    .as_deref()
                    .map(|n| (format!("{field}.note"), n)),
            );
        }
        uncited_in_text(&free, &self.cited_text(), &self.cited_numbers())
    }

    /// The distance or parallax, by field.
    fn researched(&self) -> Vec<(&'static str, &Researched)> {
        [("distance", &self.distance), ("parallax", &self.parallax)]
            .into_iter()
            .filter_map(|(f, r)| Some((f, r.as_ref()?)))
            .collect()
    }

    /// What the file's citations show: the star's names, the papers' references and every
    /// quoted text.
    fn cited_text(&self) -> Vec<&str> {
        let mut out: Vec<&str> = std::iter::once(self.name.as_str())
            .chain(self.alternative_names.iter().map(String::as_str))
            .chain(self.papers.iter().map(|p| p.reference.as_str()))
            .collect();
        let cites = self
            .researched()
            .into_iter()
            .flat_map(|(_, r)| {
                std::iter::once(&r.value.cite).chain(r.value.uncertainty_cite.as_deref())
            })
            .chain(self.membership.as_ref());
        for cite in cites {
            out.extend(shown(cite));
        }
        out
    }

    /// The numbers the file cites: the value, its uncertainty and its form as published.
    fn cited_numbers(&self) -> Vec<f64> {
        self.researched()
            .into_iter()
            .flat_map(|(_, r)| {
                let mut n = vec![r.value.value()];
                n.extend(r.value.uncertainty_numbers());
                n
            })
            .collect()
    }

    /// The papers the star's values come from, the paper of its distance first, each once.
    fn references(&self) -> Vec<PaperReference> {
        let mut ids: Vec<&str> = vec![];
        for (_, r) in self.researched() {
            ids.push(&r.value.cite.paper);
            ids.extend(
                r.value
                    .uncertainty_cite
                    .as_deref()
                    .map(|c| c.paper.as_str()),
            );
        }
        ids.extend(self.membership.as_ref().map(|c| c.paper.as_str()));
        let mut seen = vec![];
        ids.into_iter()
            .filter(|id| {
                let new = !seen.contains(id);
                seen.push(id);
                new
            })
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
        let distances = match &self.distance {
            Some(d) => vec![
                Distance::new(
                    d.value.quantity,
                    None,
                    Unit::Parsec,
                    d.value.kind,
                    (d.distance_kind, Method::Other(d.method.clone())),
                    Measure::Euclidean,
                    None,
                )
                .map_err(|e| e.to_string())?,
            ],
            None => vec![],
        };
        let parallax = match &self.parallax {
            Some(p) => Some(
                Value::normalize(p.value.quantity, Unit::Degree, p.value.kind)
                    .map_err(|e| e.to_string())?,
            ),
            None => None,
        };
        Ok(Some(CatalogueEntry {
            designation,
            name: Some(self.name.clone()),
            position: None,
            distances,
            redshifts: vec![],
            velocity: None,
            parallax,
            proper_motion: None,
            magnitude: None,
            colours: vec![],
            spectral_type: None,
            group_id: None,
            supergalactic_velocity: None,
            flags: BTreeMap::new(),
            references: Some(self.references().into_boxed_slice()),
        }))
    }
}

/// Reads every star file in `dir`, in file-name order, refusing on the first that cannot be
/// used. A file is named after its star: `<kebab-slug>.yaml`.
///
/// # Errors
///
/// Fails when the directory cannot be read, a file name is not a kebab-case slug, two files
/// give one HIP number, or a star fails [`StarFile::read`].
pub fn read_all(dir: &Path) -> Result<Vec<(PathBuf, StarFile)>, StarError> {
    let mut hips = BTreeMap::new();
    let mut out = vec![];
    for file in read_dir(dir)? {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !is_slug(stem) {
            return Err(FileError::new(
                &file,
                "(file name)",
                "is not a kebab-case slug",
            ));
        }
        let star = StarFile::read(&file)?;
        if let Some(other) = hips.insert(star.hip, stem.to_owned()) {
            return Err(FileError::new(
                &file,
                "hip",
                format!("is also the star of {other}"),
            ));
        }
        out.push((file, star));
    }
    Ok(out)
}

/// Every problem of the star files in `dir`, one line each, in file-name order: each file's
/// first problem, and a HIP number two files give. Empty when every file can be used.
///
/// # Errors
///
/// Fails when the directory cannot be read.
pub fn problems(dir: &Path) -> Result<Vec<String>, StarError> {
    let mut hips: BTreeMap<u32, String> = BTreeMap::new();
    let mut out = vec![];
    for file in read_dir(dir)? {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !is_slug(stem) {
            out.push(FileError::new(&file, "(file name)", "is not a kebab-case slug").to_string());
            continue;
        }
        match StarFile::read(&file) {
            Ok(star) => {
                if let Some(other) = hips.insert(star.hip, stem.to_owned()) {
                    let twice = format!("is also the star of {other}");
                    out.push(FileError::new(&file, "hip", twice).to_string());
                }
            }
            Err(e) => out.push(e.to_string()),
        }
    }
    Ok(out)
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
