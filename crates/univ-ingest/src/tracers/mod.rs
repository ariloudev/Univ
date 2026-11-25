//! Researched arm tracers: one YAML file per tracer under the source's directory, for the masers
//! and molecular clouds that trace the Milky Way's spiral arms beyond the reach of Reid et al.
//! 2019 (record-a-researched-arm-tracer), in which every number is a cited value read from a
//! paper, as in a structure file.
//!
//! A tracer file names the tracer, the kind of object it is, the arm its paper assigns it to
//! with the citation that does, and its sky position as the paper gives it, equatorial or
//! galactic; then its trigonometric parallax, the distance its paper adopts, or both. A
//! parallax is measured; a distance that rests on a rotation curve, kinematic or combined with
//! the parallax, is inferred, and states the rotation curve and the Galactic Centre's distance
//! it assumes. The LSR velocity is kept in the file, not in the entry: it is a velocity relative
//! to the local standard of rest, not heliocentric.
//!
//! Each file becomes a catalogue entry of its source, identified by its file's slug, with its
//! arm as the flag `Arm`, through which the source's memberships make it a member of each
//! structure of that arm.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{
    CatalogueEntry, Flags, NativePosition, PaperReference, Position, PositionUncertainty,
};
use univ_model::ids::Designation;
use univ_model::object::ObjectKind;
use univ_model::position::{Epoch, Frame};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty, Value};

use crate::normalize::{NormalizationReport, NormalizeError, Normalized};
use crate::source::SourceDefinition;
use crate::stars::Researched;
use crate::structures::{Arm, Citation, Cited, CitedFile, Evidence, Paper, Problem, problem};
use crate::yaml::{FileError, read_checked, read_dir, slug_of};

/// The flag that holds a tracer's arm, which the source's memberships read.
pub const ARM_FLAG: &str = "Arm";

/// A tracer as its file describes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracerFile {
    pub name: String,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    /// What the tracer is: a star-forming region for a maser, a molecular cloud for a cloud seen
    /// in CO.
    pub kind: ObjectKind,
    /// The arm the paper assigns the tracer to.
    pub arm: Arm,
    /// Where the paper assigns it.
    pub arm_cite: Citation,
    pub papers: Vec<Paper>,
    pub position: TracerPosition,
    /// The tracer's trigonometric parallax.
    #[serde(default)]
    pub parallax: Option<Researched>,
    /// The distance the paper adopts for it.
    #[serde(default)]
    pub distance: Option<Researched>,
    /// Its velocity relative to the local standard of rest, kept in the file only.
    #[serde(default)]
    pub velocity_lsr: Option<Cited>,
    pub notes: String,
}

/// A sky position as a paper gives it: galactic `l` and `b`, or equatorial `ra` and `dec` in
/// the ICRS or FK5 J2000.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracerPosition {
    pub frame: Frame,
    #[serde(default)]
    pub l: Option<Cited>,
    #[serde(default)]
    pub b: Option<Cited>,
    #[serde(default)]
    pub ra: Option<Cited>,
    #[serde(default)]
    pub dec: Option<Cited>,
}

impl TracerPosition {
    /// Its longitude and latitude with their fields, those its frame takes; none where the
    /// position gives another pair or both.
    fn pair(&self) -> Option<[(&'static str, &Cited); 2]> {
        match (self.frame, &self.l, &self.b, &self.ra, &self.dec) {
            (Frame::Galactic, Some(l), Some(b), None, None) => {
                Some([("position.l", l), ("position.b", b)])
            }
            (Frame::Icrs | Frame::Fk5J2000, None, None, Some(ra), Some(dec)) => {
                Some([("position.ra", ra), ("position.dec", dec)])
            }
            _ => None,
        }
    }
}

/// A tracer file that cannot be used, naming the file, the field and what is wrong.
pub type TracerError = FileError;

impl CitedFile for TracerFile {
    fn papers(&self) -> &[Paper] {
        &self.papers
    }
}

impl TracerFile {
    /// Reads and checks the tracer in `file`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, on YAML that does not parse, on a number that cites
    /// nothing or that its quoted sentence or table cell does not hold, and on the checks of
    /// [`TracerFile::check`].
    pub fn read(file: &Path) -> Result<Self, TracerError> {
        read_checked(file, Self::check)
    }

    /// The checks serde cannot make: the papers stand and every citation names one of them;
    /// the tracer is a maser's region or a molecular cloud; its position is a pair its frame
    /// takes, in degrees; it has a positive parallax, measured, or a positive distance,
    /// inferred and stating what it assumes, or both; and every number of the free text is cited.
    fn check(&self) -> Result<(), Problem> {
        if self.name.trim().is_empty() || self.notes.trim().is_empty() {
            return problem("name", "the name and the notes must not be empty");
        }
        if !matches!(
            self.kind,
            ObjectKind::StarFormingRegion | ObjectKind::MolecularCloud
        ) {
            return problem(
                "kind",
                "an arm tracer is a star_forming_region or a molecular_cloud",
            );
        }
        self.check_papers()?;
        self.check_citation("arm_cite", &self.arm_cite, None)?;
        let Some(pair) = self.position.pair() else {
            return problem(
                "position",
                "a galactic position gives l and b, an equatorial one ra and dec, and nothing else",
            );
        };
        for (field, c) in pair.iter().copied().chain(self.velocity()) {
            self.check_citation(field, &c.cite, Some(c))?;
            self.check_value(field, c)?;
        }
        for (field, c) in pair {
            if c.unit() != Unit::Degree {
                return problem(field, "is given in degrees");
            }
        }
        if self.parallax.is_none() && self.distance.is_none() {
            return problem("distance", "give a parallax, a distance or both");
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
            let symmetric = matches!(p.value.quantity.uncertainty, Uncertainty::Symmetric { .. });
            if p.value.unit() != Unit::Milliarcsecond
                || p.distance_kind != DistanceKind::Parallax
                || p.value.kind != KindOfValue::Measured
                || !symmetric
            {
                return problem(
                    "parallax",
                    "a trigonometric parallax is measured, of kind parallax, in milliarcseconds, \
                     with a symmetric error",
                );
            }
        }
        if let Some(d) = &self.distance {
            if !matches!(d.value.unit(), Unit::Parsec | Unit::Kiloparsec) {
                return problem("distance.unit", "is given in parsecs or kiloparsecs");
            }
            if d.value.kind != KindOfValue::Inferred || d.value.assumes.is_none() {
                return problem(
                    "distance",
                    "a distance resting on a rotation curve is inferred, and states in assumes \
                     the rotation curve and the Galactic Centre's distance it takes",
                );
            }
        }
        self.evidence().check()
    }

    /// The LSR velocity, by field, where the file gives one.
    fn velocity(&self) -> Option<(&'static str, &Cited)> {
        self.velocity_lsr.as_ref().map(|v| ("velocity_lsr", v))
    }

    /// The parallax and the distance, by field.
    fn researched(&self) -> impl Iterator<Item = (&'static str, &Researched)> {
        [("parallax", &self.parallax), ("distance", &self.distance)]
            .into_iter()
            .filter_map(|(f, r)| Some((f, r.as_ref()?)))
    }

    /// The file's free text beside what its citations show.
    fn evidence(&self) -> Evidence<'_> {
        let mut evidence = Evidence::default();
        evidence.text(&self.name);
        for name in &self.alternative_names {
            evidence.text(name);
        }
        for paper in &self.papers {
            evidence.text(&paper.reference);
        }
        evidence.cite(&self.arm_cite);
        evidence.free("notes", &self.notes);
        for (field, c) in self
            .position
            .pair()
            .into_iter()
            .flatten()
            .chain(self.velocity())
        {
            evidence.value(field, c);
        }
        for (field, r) in self.researched() {
            evidence.value(field, &r.value);
            evidence.free(format!("{field}.method"), &r.method);
        }
        evidence
    }

    /// The papers the tracer's values come from, each once, in the order the file lists them.
    fn references(&self) -> Vec<PaperReference> {
        self.papers
            .iter()
            .map(|p| PaperReference {
                citation: p.reference.clone(),
                doi: p.doi.clone(),
                bibcode: p.bibcode.clone(),
            })
            .collect()
    }

    /// The tracer's catalogue entry in `definition`'s source, identified by `slug`.
    ///
    /// # Errors
    ///
    /// Fails when the tracer's kind or frame is not its source's, or a value cannot stand as a
    /// univ-model value, naming why.
    pub fn entry(
        &self,
        definition: &SourceDefinition,
        slug: &str,
    ) -> Result<CatalogueEntry, String> {
        if self.position.frame != definition.frame {
            return Err(format!(
                "the tracer's position is in {:?}, but its source's positions are in {:?}",
                self.position.frame, definition.frame
            ));
        }
        if definition.object_kind != Some(self.kind) {
            return Err(format!(
                "the tracer is a {:?}, but its source lists {:?}",
                self.kind, definition.object_kind
            ));
        }
        let designation =
            Designation::new(definition.id.clone(), slug.to_owned()).map_err(|e| e.to_string())?;
        let [(_, lon), (_, lat)] = self
            .position
            .pair()
            .ok_or_else(|| "the position gives no pair its frame takes".to_owned())?;
        let position = Position::new(
            NativePosition {
                frame: self.position.frame,
                lon: lon.value(),
                lat: lat.value(),
                unit: Unit::Degree,
                epoch: Epoch::J2000,
            },
            PositionUncertainty::NoneReported,
        )
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
        let flags: Flags = std::iter::once((ARM_FLAG, self.arm.slug().to_owned()))
            .chain(self.parallax.iter().map(|p| ("method", p.method.clone())))
            .collect();
        let entry = CatalogueEntry {
            cross_ids: Flags::default(),
            designation,
            name: Some(self.name.as_str().into()),
            position: Some(position),
            distances: distances.into_iter().collect(),
            redshifts: thin_vec::thin_vec![],
            velocity: None,
            parallax: parallax.map(Box::new),
            proper_motion: None,
            magnitude: None,
            colours: thin_vec::thin_vec![],
            ages: thin_vec::ThinVec::new(),
            spectral_type: None,
            group_id: None,
            supergalactic_velocity: None,
            flags,
            references: self.references().into(),
        };
        entry.check().map_err(|e| e.to_string())?;
        Ok(entry)
    }
}

/// Reads every tracer file in `dir`, in file-name order, with its slug, refusing on the first
/// that cannot be used. A file is named after its tracer: `<kebab-slug>.yaml`.
///
/// # Errors
///
/// Fails when the directory cannot be read, a file name is not a kebab-case slug, or a tracer
/// fails [`TracerFile::read`].
pub fn read_all(dir: &Path) -> Result<Vec<(PathBuf, String, TracerFile)>, TracerError> {
    read_dir(dir)?
        .into_iter()
        .map(|file| {
            let slug = slug_of(&file)?.to_owned();
            let tracer = TracerFile::read(&file)?;
            Ok((file, slug, tracer))
        })
        .collect()
}

/// Every problem of the tracer files in `dir`, one line each, in file-name order: each file's
/// first problem. Empty when every file can be used.
///
/// # Errors
///
/// Fails when the directory cannot be read.
pub fn problems(dir: &Path) -> Result<Vec<String>, TracerError> {
    Ok(read_dir(dir)?
        .into_iter()
        .filter_map(|file| {
            slug_of(&file)
                .and_then(|_| TracerFile::read(&file))
                .err()
                .map(|e| e.to_string())
        })
        .collect())
}

/// The researched arm tracers of `definition`, read from the tracer files in `dir`: an entry
/// for each tracer.
///
/// # Errors
///
/// Fails on the first tracer file that cannot be used.
pub fn normalize(definition: &SourceDefinition, dir: &Path) -> Result<Normalized, NormalizeError> {
    let refused = |file: PathBuf, error: String| NormalizeError::Table {
        source_id: definition.id.clone(),
        file,
        error,
    };
    let tracers = read_all(dir).map_err(|e| refused(dir.to_owned(), e.to_string()))?;
    let mut report = NormalizationReport::default();
    let mut entries = vec![];
    for (file, slug, tracer) in tracers {
        entries.push(
            tracer
                .entry(definition, &slug)
                .map_err(|e| refused(file, e))?,
        );
        report.published += 1;
    }
    Ok(Normalized {
        source: definition.id.clone(),
        entries,
        report,
    })
}
