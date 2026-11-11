//! The checks of a structure file that span its parts: each cited number is held by its
//! citation, with its uncertainty; angles lie in range; a length in Mpc names the cosmology it
//! assumes; and every number written in free text (notes, how a figure was read, why a structure
//! is unplaced, a dispute's claim) is one the file cites.

use univ_model::units::{Dimension, Unit};

use super::citation::{Citation, Locator};
use super::cited::{Assumes, Cited};
use super::geometry::is_length;
use super::members::{ColumnName, MemberSource};
use super::numbers::{has_magnitude, has_number, numbers, stated_numbers};
use super::spiral::Extent;
use super::{Dispute, Paper, Status, Structure, StructureGeometry, StructureKind};
use crate::source::ScaleDomain;
use std::collections::HashSet;
use univ_model::ids::is_slug;

pub(crate) type Problem = (String, String);

pub(crate) fn problem(field: &str, what: impl Into<String>) -> Result<(), Problem> {
    Err((field.to_owned(), what.into()))
}

/// The texts a citation shows: the quoted sentence, the table's number, row, column and cell,
/// or the figure's number and how it was read.
pub(crate) fn shown(cite: &Citation) -> Vec<&str> {
    match &cite.locator {
        Locator::Sentence { quote, .. } => vec![quote],
        Locator::Table {
            number,
            row,
            column,
            cell,
        } => [Some(number), Some(row), Some(column), cell.as_ref()]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect(),
        Locator::Figure { number, how } => vec![number, how],
    }
}

/// A file of values cited from its papers, such as a structure file or a star file: the checks
/// of its citations against its papers, which every such file shares.
pub(crate) trait CitedFile {
    /// The papers the file cites.
    fn papers(&self) -> &[Paper];

    /// The papers' ids are unique lowercase slugs, and each paper has a DOI or a bibcode, its
    /// reference and how it was read. Gives the ids.
    fn check_papers(&self) -> Result<HashSet<&str>, Problem> {
        let mut ids = HashSet::new();
        for (i, paper) in self.papers().iter().enumerate() {
            if !is_slug(&paper.id) || !ids.insert(paper.id.as_str()) {
                return Err((
                    format!("papers[{i}].id"),
                    "is not a unique lowercase slug".to_owned(),
                ));
            }
            if paper.doi.is_none() && paper.bibcode.is_none() {
                return Err((
                    format!("papers[{i}]"),
                    "needs a DOI or a bibcode".to_owned(),
                ));
            }
            if paper.reference.trim().is_empty() || paper.read.trim().is_empty() {
                return Err((
                    format!("papers[{i}]"),
                    "needs its reference and how it was read".to_owned(),
                ));
            }
        }
        Ok(ids)
    }

    /// A citation names one of the file's papers and, for a value cited by a sentence, the
    /// sentence holds the value's number.
    fn check_citation(
        &self,
        field: &str,
        cite: &Citation,
        value: Option<&Cited>,
    ) -> Result<(), Problem> {
        self.check_paper(&format!("{field}.cite"), cite)?;
        if let (Some(value), Locator::Sentence { quote, .. }) = (value, &cite.locator)
            && !value.appears_in(quote)
        {
            let shown = value
                .as_published
                .clone()
                .unwrap_or_else(|| value.value().to_string());
            return problem(
                field,
                format!("the number {shown} does not appear in the sentence it cites"),
            );
        }
        Ok(())
    }

    /// The checks of one cited value, beyond its citation's paper.
    fn check_value(&self, field: &str, value: &Cited) -> Result<(), Problem> {
        if let Locator::Table {
            cell: Some(cell), ..
        } = &value.cite.locator
            && !value.appears_in(cell)
        {
            return problem(
                field,
                format!(
                    "the number {} does not appear in the cell it quotes",
                    value.value()
                ),
            );
        }
        if let Some(cite) = &value.uncertainty_cite {
            self.check_paper(&format!("{field}.uncertainty_cite"), cite)?;
        }
        if let Some(x) = value.uncertainty_not_shown() {
            return problem(
                field,
                format!(
                    "the uncertainty {x} does not appear in its citation: quote it in the \
                     sentence or table cell, or give the uncertainty its own uncertainty_cite"
                ),
            );
        }
        if value.unit() == Unit::Megaparsec && value.assumes.is_none() {
            return problem(
                field,
                "a length in Mpc depends on the cosmology its paper used: give assumes",
            );
        }
        if let Some(assumes) = &value.assumes {
            self.check_assumes(&format!("{field}.assumes"), assumes)?;
        }
        let angle = value.unit().factor_to(Unit::Degree).ok();
        let leaf = field.rsplit('.').next().unwrap_or(field);
        if let Some(factor) = angle {
            in_range(field, leaf, value.value() * factor)?;
        }
        Ok(())
    }

    /// Each dispute states a claim and cites one of the file's papers, and every number the
    /// claim states, beyond the numbers that name a table, section or catalogue entry, appears
    /// in its own citation.
    fn check_disputes(&self, disputes: &[Dispute]) -> Result<(), Problem> {
        for (i, dispute) in disputes.iter().enumerate() {
            let field = format!("disputes[{i}]");
            if dispute.claim.trim().is_empty() {
                return problem(&field, "states no claim");
            }
            self.check_citation(&field, &dispute.cite, None)?;
            let texts = shown(&dispute.cite);
            if let Some(x) = stated_numbers(&dispute.claim)
                .into_iter()
                .find(|x| !texts.iter().any(|t| has_number(t, *x)))
            {
                return problem(
                    &format!("{field}.claim"),
                    format!("the number {x} does not appear in the sentence it cites"),
                );
            }
        }
        Ok(())
    }

    fn check_paper(&self, field: &str, cite: &Citation) -> Result<(), Problem> {
        if self.papers().iter().any(|p| p.id == cite.paper) {
            Ok(())
        } else {
            problem(
                &format!("{field}.paper"),
                format!("{} is not among the papers", cite.paper),
            )
        }
    }

    /// A cited cosmology names a paper of the file, and its quoted sentence holds every number
    /// of the cosmology as written.
    fn check_assumes(&self, field: &str, assumes: &Assumes) -> Result<(), Problem> {
        let Some(cite) = &assumes.cite else {
            return Ok(());
        };
        self.check_paper(&format!("{field}.cite"), cite)?;
        let texts = shown(cite);
        if let Some(x) = numbers(&assumes.stated)
            .into_iter()
            .find(|x| !texts.iter().any(|t| has_number(t, *x)))
        {
            return problem(
                field,
                format!("the number {x} of the cosmology does not appear in its citation"),
            );
        }
        Ok(())
    }
}

impl CitedFile for Structure {
    fn papers(&self) -> &[Paper] {
        &self.papers
    }
}

impl Structure {
    /// Extents name three of the file's values, each a positive length or velocity, about a
    /// position given in a frame, and only for a structure placed at its position.
    pub(super) fn check_extents(&self) -> Result<(), Problem> {
        let Some(extents) = &self.extents else {
            return Ok(());
        };
        if self.geometry.is_some()
            || self
                .position
                .as_ref()
                .and_then(super::geometry::Coordinates::frame)
                .is_none()
        {
            return problem(
                "extents",
                "extents lie about a position given in a frame, for a structure placed at it",
            );
        }
        for (axis, name) in ["x", "y", "z"].into_iter().zip(extents.names()) {
            let field = format!("extents.{axis}");
            let Some(value) = self.values.get(name) else {
                return problem(
                    &field,
                    format!("names {name}, which is not among the values"),
                );
            };
            if !(is_length(value.unit()) || value.unit().dimension() == Dimension::Velocity) {
                return problem(
                    &field,
                    format!("names {name}, which is not a length or a velocity"),
                );
            }
            if value.value() <= 0.0 {
                return problem(&field, format!("names {name}, which is not positive"));
            }
        }
        Ok(())
    }

    /// The member table: a column of lengths in Mpc names its cosmology, and every longitude
    /// and latitude is in range.
    pub(super) fn check_members(&self) -> Result<(), Problem> {
        let Some(StructureGeometry::Members(m)) = &self.geometry else {
            return Ok(());
        };
        for (i, column) in m.columns.iter().enumerate() {
            let field = format!("geometry.members.columns[{i}]");
            if column.unit == Unit::Megaparsec && column.assumes.is_none() {
                return problem(
                    &field,
                    "a length in Mpc depends on the cosmology its paper used: give assumes",
                );
            }
            if let Some(assumes) = &column.assumes {
                self.check_assumes(&format!("{field}.assumes"), assumes)?;
            }
            let leaf = match column.name {
                ColumnName::Lon => "lon",
                ColumnName::Lat => "lat",
                _ => continue,
            };
            let factor = column.unit.factor_to(Unit::Degree).unwrap_or(1.0);
            for row in &m.rows {
                in_range(
                    &format!("geometry.members.rows[{}]", row.name),
                    leaf,
                    row.values[i] * factor,
                )?;
            }
        }
        Ok(())
    }

    /// A log spiral is a model spiral arm of the Galaxy: its kind names its extent, a spiral arm
    /// over its fitted range or that arm's extrapolation, its status is `model` and it is listed
    /// among the Galaxy tracers; and a spiral arm kind takes a log spiral.
    pub(super) fn check_spiral(&self) -> Result<(), Problem> {
        let spiral = match &self.geometry {
            Some(StructureGeometry::LogSpiral(l)) => Some(l.extent),
            _ => None,
        };
        let kind = match self.kind {
            StructureKind::SpiralArm => Some(Extent::Fitted),
            StructureKind::SpiralArmExtrapolation => Some(Extent::Extrapolated),
            _ => None,
        };
        if spiral != kind {
            return problem(
                "geometry",
                "a spiral_arm takes a log spiral over its fitted range, a \
                 spiral_arm_extrapolation one extrapolated, and no other kind takes one",
            );
        }
        if spiral.is_some() && self.status != Status::Model {
            return problem("status", "a log spiral is computed from a model: model");
        }
        if spiral.is_some() && self.scale_domain != ScaleDomain::GalaxyTracers {
            return problem(
                "scale_domain",
                "a spiral arm of the Galaxy is listed among the Galaxy tracers",
            );
        }
        Ok(())
    }

    /// Every number in the notes, in how a figure was read, in why the structure is unplaced
    /// and in a value's note is one the file cites.
    pub(super) fn check_free_text(&self) -> Result<(), Problem> {
        let mut evidence = Evidence::default();
        for paper in &self.papers {
            evidence.text(&paper.reference);
        }
        for dispute in &self.disputes {
            evidence.cite(&dispute.cite);
        }
        evidence.free("notes", &self.notes);
        if let Some(why) = &self.unplaced {
            evidence.free("unplaced", why);
        }
        if let Some(StructureGeometry::Members(m)) = &self.geometry {
            match &m.source {
                MemberSource::Table { number, .. } => {
                    evidence.text(number);
                    evidence.numbers(m.rows.iter().flat_map(|r| r.values.iter().copied()));
                }
                MemberSource::Figure { number, how, .. } => {
                    evidence.text(number);
                    evidence.free("geometry.members.source.how", how);
                }
            }
        }
        for (field, value) in self.cited_values() {
            evidence.value(&field, value);
        }
        evidence.check()
    }
}

/// What a file's free text may rest on: the texts its citations show and the numbers it cites,
/// beside the free text itself, by field.
#[derive(Default)]
pub(crate) struct Evidence<'a> {
    shown: Vec<&'a str>,
    numbers: Vec<f64>,
    free: Vec<(String, &'a str)>,
}

impl<'a> Evidence<'a> {
    /// A text the file cites as it stands, such as a paper's reference or the star's name.
    pub(crate) fn text(&mut self, text: &'a str) {
        self.shown.push(text);
    }

    pub(crate) fn numbers(&mut self, numbers: impl IntoIterator<Item = f64>) {
        self.numbers.extend(numbers);
    }

    /// Free text, whose every number must be cited.
    pub(crate) fn free(&mut self, field: impl Into<String>, text: &'a str) {
        self.free.push((field.into(), text));
    }

    /// What a citation shows: of a figure, only its number, since how it was read is free text.
    pub(crate) fn cite(&mut self, cite: &'a Citation) {
        match &cite.locator {
            Locator::Figure { number, .. } => self.shown.push(number),
            _ => self.shown.extend(shown(cite)),
        }
    }

    /// A cited value: its citations, its number, uncertainty and form as published, and its
    /// free text, how a figure was read and its note.
    pub(crate) fn value(&mut self, field: &str, value: &'a Cited) {
        self.cite(&value.cite);
        if let Some(cite) = value.uncertainty_cite.as_deref() {
            self.cite(cite);
        }
        if let Some(cite) = value.assumes.as_deref().and_then(|a| a.cite.as_ref()) {
            self.cite(cite);
        }
        self.numbers.push(value.value());
        self.numbers.extend(value.uncertainty_numbers());
        self.numbers
            .extend(value.as_published.iter().flat_map(|p| numbers(p)));
        if let Locator::Figure { how, .. } = &value.cite.locator {
            self.free(format!("{field}.cite.how"), how);
        }
        if let Some(note) = &value.note {
            self.free(format!("{field}.note"), note);
        }
    }

    /// Every number of the free text is shown by a citation or among the cited numbers.
    pub(crate) fn check(&self) -> Result<(), Problem> {
        uncited_in_text(&self.free, &self.shown, &self.numbers)
    }
}

/// Every number written in `free` text is one the file cites: shown in `corpus`, the text of
/// its citations, or among its `cited` numbers.
#[expect(
    clippy::float_cmp,
    reason = "the same number as written, not a near one"
)]
fn uncited_in_text(free: &[(String, &str)], corpus: &[&str], cited: &[f64]) -> Result<(), Problem> {
    let known = |x: f64| {
        corpus.iter().any(|t| has_magnitude(t, x)) || cited.iter().any(|n| n.abs() == x.abs())
    };
    for (field, text) in free {
        if let Some(x) = stated_numbers(text).into_iter().find(|x| !known(*x)) {
            return problem(
                field,
                format!(
                    "the number {x} is not one the file cites: quote it in a citation, or leave \
                     it out"
                ),
            );
        }
    }
    Ok(())
}

/// A latitude lies in [-90°, 90°] and a longitude in [0°, 360°), named by the last part of
/// their field, such as `lat` or `lon[1]`.
fn in_range(field: &str, leaf: &str, degrees: f64) -> Result<(), Problem> {
    let named = |name: &str| leaf == name || leaf.starts_with(&format!("{name}["));
    if named("lat") && !(-90.0..=90.0).contains(&degrees) {
        return problem(
            field,
            format!("the latitude {degrees}° lies outside -90° to 90°"),
        );
    }
    if named("lon") && !(0.0..360.0).contains(&degrees) {
        return problem(
            field,
            format!("the longitude {degrees}° lies outside 0° to 360°"),
        );
    }
    Ok(())
}
