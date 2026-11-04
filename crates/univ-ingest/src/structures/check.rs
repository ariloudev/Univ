//! The checks of a structure file that span its parts: each cited number is held by its
//! citation, with its uncertainty; angles lie in range; a length in Mpc names the cosmology it
//! assumes; and every number written in free text (notes, how a figure was read, why a structure
//! is unplaced, a dispute's claim) is one the file cites.

use univ_model::units::{Dimension, Unit};

use super::citation::{Citation, Locator};
use super::cited::{Assumes, Cited};
use super::members::{ColumnName, MemberSource};
use super::numbers::{has_magnitude, has_number, numbers, stated_numbers};
use super::{Structure, StructureGeometry};

type Problem = (String, String);

fn problem(field: &str, what: impl Into<String>) -> Result<(), Problem> {
    Err((field.to_owned(), what.into()))
}

/// The texts a citation shows: the quoted sentence, the table's number, row, column and cell,
/// or the figure's number and how it was read.
fn shown(cite: &Citation) -> Vec<&str> {
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

impl Structure {
    /// The checks of one cited value, beyond its citation's paper.
    pub(super) fn check_value(&self, field: &str, value: &Cited) -> Result<(), Problem> {
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

    pub(super) fn check_paper(&self, field: &str, cite: &Citation) -> Result<(), Problem> {
        if self.papers.iter().any(|p| p.id == cite.paper) {
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

    /// The member table: a column of lengths in Mpc names its cosmology, and every longitude
    /// and latitude is in range.
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
            let dimension = value.unit().dimension();
            if !matches!(
                dimension,
                Dimension::Length | Dimension::HubbleScaledLength | Dimension::Velocity
            ) {
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

    /// Every number of a dispute's claim appears in the claim's own citation, and every number
    /// in the notes, in how a figure was read, in why the structure is unplaced and in a value's
    /// note is one the file cites.
    #[expect(
        clippy::float_cmp,
        reason = "the same number as written, not a near one"
    )]
    pub(super) fn check_free_text(&self) -> Result<(), Problem> {
        for (i, dispute) in self.disputes.iter().enumerate() {
            let texts = shown(&dispute.cite);
            if let Some(x) = numbers(&dispute.claim)
                .into_iter()
                .find(|x| !texts.iter().any(|t| has_number(t, *x)))
            {
                return problem(
                    &format!("disputes[{i}].claim"),
                    format!("the number {x} does not appear in the sentence it cites"),
                );
            }
        }
        let corpus = self.cited_text();
        let cited_numbers = self.cited_numbers();
        let known = |x: f64| {
            corpus.iter().any(|t| has_magnitude(t, x))
                || cited_numbers.iter().any(|n| n.abs() == x.abs())
        };
        for (field, text) in self.free_text() {
            if let Some(x) = stated_numbers(text).into_iter().find(|x| !known(*x)) {
                return problem(
                    &field,
                    format!(
                        "the number {x} is not one the file cites: quote it in a citation, or \
                         leave it out"
                    ),
                );
            }
        }
        Ok(())
    }

    /// The free text of the file, by field.
    fn free_text(&self) -> Vec<(String, &str)> {
        let mut out = vec![("notes".to_owned(), self.notes.as_str())];
        if let Some(why) = &self.unplaced {
            out.push(("unplaced".to_owned(), why));
        }
        if let Some(StructureGeometry::Members(m)) = &self.geometry
            && let MemberSource::Figure { how, .. } = &m.source
        {
            out.push(("geometry.members.source.how".to_owned(), how));
        }
        for (field, value) in self.cited_values() {
            if let Locator::Figure { how, .. } = &value.cite.locator {
                out.push((format!("{field}.cite.how"), how));
            }
            if let Some(note) = &value.note {
                out.push((format!("{field}.note"), note));
            }
        }
        out
    }

    /// What the file's citations show: the papers' references, every quoted sentence, the
    /// table and figure locators, and the cosmologies cited.
    fn cited_text(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.papers.iter().map(|p| p.reference.as_str()).collect();
        let mut cites: Vec<&Citation> = self.disputes.iter().map(|d| &d.cite).collect();
        for (_, value) in self.cited_values() {
            cites.push(&value.cite);
            cites.extend(value.uncertainty_cite.as_deref());
            if let Some(cite) = value.assumes.as_deref().and_then(|a| a.cite.as_ref()) {
                cites.push(cite);
            }
        }
        for cite in cites {
            out.extend(match &cite.locator {
                Locator::Figure { number, .. } => vec![number.as_str()],
                _ => shown(cite),
            });
        }
        if let Some(StructureGeometry::Members(m)) = &self.geometry {
            match &m.source {
                MemberSource::Table { number, .. } | MemberSource::Figure { number, .. } => {
                    out.push(number);
                }
            }
        }
        out
    }

    /// The numbers the file cites: each value, its uncertainty and its form as published, and
    /// the cells of a member table read from a table.
    fn cited_numbers(&self) -> Vec<f64> {
        let mut out = vec![];
        for (_, value) in self.cited_values() {
            out.push(value.value());
            out.extend(value.uncertainty_numbers());
            out.extend(value.as_published.iter().flat_map(|p| numbers(p)));
        }
        if let Some(StructureGeometry::Members(m)) = &self.geometry
            && matches!(m.source, MemberSource::Table { .. })
        {
            out.extend(m.rows.iter().flat_map(|r| r.values.iter().copied()));
        }
        out
    }
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
