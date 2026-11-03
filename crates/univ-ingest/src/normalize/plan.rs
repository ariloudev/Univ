//! A source definition resolved against a table's header: the position of each column a field
//! comes from, the unit it is read in after the fixes, and the readings, rules and fixes that
//! apply to it.

use std::collections::BTreeMap;

use univ_model::cosmology::Cosmology;
use univ_model::distance::{DistanceKind, Measure, Method};
use univ_model::entry::RedshiftKind;
use univ_model::position::Frame;
use univ_model::redshift::RedshiftFrame;
use univ_model::units::{Dimension, Unit};
use univ_model::value::KindOfValue;

mod fixes;

pub(super) use fixes::{FixEffect, FixPlan};

use super::NormalizeError;
use crate::source::{
    Coded, ComovingVelocity, Condition, DistanceReading, ErrorColumns, Field, FixAction,
    LeftOutReason, MAX_FIXES, RedshiftReading, Rule, ScaleDomain, SourceDefinition,
};

/// The fixes, by index, that changed a row: a bit each, so a definition has at most
/// [`MAX_FIXES`].
pub(super) type Changed = u64;

const _: () = assert!(MAX_FIXES <= Changed::BITS as usize);

/// A column as the normalizer reads it.
#[derive(Clone, Debug)]
pub(super) struct Col {
    pub index: usize,
    pub name: String,
    /// The column's unit, or the one a fix reads it in.
    pub unit: Unit,
}

/// The error columns of a value, resolved.
#[derive(Clone, Debug)]
pub(super) enum Errors {
    Symmetric(Col),
    Asymmetric {
        minus: Col,
        plus: Col,
    },
    CredibleInterval {
        low: Col,
        high: Col,
        probability: f64,
    },
}

/// A value and its error columns.
#[derive(Clone, Debug)]
pub(super) struct Measured {
    pub value: Col,
    pub error: Option<Errors>,
}

/// How a reading names its kind: once, or by a code column.
#[derive(Clone, Debug)]
pub(super) enum ByCode<T> {
    Fixed(T),
    Code {
        column: usize,
        codes: BTreeMap<String, T>,
    },
}

pub(super) struct RedshiftPlan {
    pub measured: Measured,
    pub kind: ByCode<RedshiftKind>,
    /// The frame it is published in: heliocentric, or the CMB frame.
    pub frame: RedshiftFrame,
    /// From this velocity up (km/s), a comoving distance in velocity units in this cosmology,
    /// read as a redshift of this kind of value.
    pub comoving: Option<(f64, Cosmology, KindOfValue)>,
}

pub(super) struct DistancePlan {
    pub measured: Measured,
    pub also_published: Option<Measured>,
    pub how: ByCode<(DistanceKind, Method)>,
    pub measure: Measure,
    pub kind_of_value: KindOfValue,
    pub hubble_constant: Option<f64>,
}

/// A rule resolved: the rows it matches, by the column of each condition, and what it does to
/// them.
pub(super) struct RulePlan {
    pub conditions: Vec<(usize, Condition)>,
    pub action: RuleAction,
}

pub(super) enum RuleAction {
    LeaveOut(LeftOutReason),
    /// The values read from these columns take this kind of value.
    Kind(Vec<usize>, KindOfValue),
    /// The distance of this index among the plan's distances takes this kind and method.
    Distance(usize, (DistanceKind, Method)),
}

/// Everything the row reader needs, resolved once per table.
pub(super) struct Plan {
    pub identifier: usize,
    pub name: Option<usize>,
    pub position: Option<(Col, Col)>,
    pub position_error: Option<(Col, Col)>,
    pub frame: Frame,
    pub velocity: Option<Measured>,
    pub parallax: Option<Measured>,
    pub proper_motion: Option<(Col, Col)>,
    pub magnitude: Option<Col>,
    pub group_id: Option<usize>,
    pub supergalactic: Option<[Col; 3]>,
    pub flags: Vec<Col>,
    pub redshifts: Vec<RedshiftPlan>,
    pub distances: Vec<DistancePlan>,
    pub rules: Vec<RulePlan>,
    pub fixes: Vec<FixPlan>,
    /// The fixes that leave a row out: the only ones that can have changed a row left out.
    pub leaving: Changed,
    /// Parsecs for stars, megaparsecs otherwise.
    pub distance_unit: Unit,
}

/// Finds the columns of a definition in a table's header.
struct Resolver<'a> {
    definition: &'a SourceDefinition,
    header: &'a [&'a str],
}

impl Resolver<'_> {
    fn unsupported(&self, what: impl Into<String>) -> NormalizeError {
        NormalizeError::Unsupported {
            source_id: self.definition.id.clone(),
            what: what.into(),
        }
    }

    /// The unit a fix reads the column `name` in, if one does.
    fn read_as(&self, name: &str) -> Option<Unit> {
        self.definition.fixes.iter().find_map(|f| match &f.action {
            Some(FixAction::ReadAs { columns, unit }) if columns.iter().any(|c| c == name) => {
                Some(*unit)
            }
            _ => None,
        })
    }

    fn col(&self, name: &str) -> Result<Col, NormalizeError> {
        let index = self.header.iter().position(|h| *h == name).ok_or_else(|| {
            NormalizeError::MissingColumn {
                source_id: self.definition.id.clone(),
                column: name.to_owned(),
            }
        })?;
        let unit = self
            .definition
            .columns
            .iter()
            .find(|c| c.column == name)
            .map_or(Unit::Dimensionless, |c| c.unit);
        Ok(Col {
            index,
            name: name.to_owned(),
            unit: self.read_as(name).unwrap_or(unit),
        })
    }

    fn index(&self, name: &str) -> Result<usize, NormalizeError> {
        self.col(name).map(|c| c.index)
    }

    fn field(&self, field: Field) -> Result<Option<Col>, NormalizeError> {
        self.definition
            .columns
            .iter()
            .find(|c| c.field == field)
            .map(|c| self.col(&c.column))
            .transpose()
    }

    fn pair(&self, a: Field, b: Field) -> Result<Option<(Col, Col)>, NormalizeError> {
        Ok(self.field(a)?.zip(self.field(b)?))
    }

    fn measured(&self, value: Field, error: Field) -> Result<Option<Measured>, NormalizeError> {
        let Some(value) = self.field(value)? else {
            return Ok(None);
        };
        let error = self.field(error)?.map(Errors::Symmetric);
        Ok(Some(Measured { value, error }))
    }

    fn errors(&self, e: &ErrorColumns) -> Result<Errors, NormalizeError> {
        Ok(match e {
            ErrorColumns::Symmetric(c) => Errors::Symmetric(self.col(c)?),
            ErrorColumns::Asymmetric { minus, plus } => Errors::Asymmetric {
                minus: self.col(minus)?,
                plus: self.col(plus)?,
            },
            ErrorColumns::CredibleInterval {
                low,
                high,
                probability,
            } => Errors::CredibleInterval {
                low: self.col(low)?,
                high: self.col(high)?,
                probability: *probability,
            },
        })
    }

    fn reading(
        &self,
        value: &str,
        error: Option<&ErrorColumns>,
    ) -> Result<Measured, NormalizeError> {
        Ok(Measured {
            value: self.col(value)?,
            error: error.map(|e| self.errors(e)).transpose()?,
        })
    }

    fn redshift(&self, r: &RedshiftReading) -> Result<RedshiftPlan, NormalizeError> {
        let kind = match (&r.kind, &r.by_code) {
            (Some(kind), _) => ByCode::Fixed(*kind),
            (None, Some(codes)) => ByCode::Code {
                column: self.index(&codes.column)?,
                codes: codes.codes.clone(),
            },
            (None, None) => return Err(self.unsupported("a redshift has no kind")),
        };
        let error = r.error.clone().map(ErrorColumns::Symmetric);
        let comoving = r.comoving_velocity.as_ref().map(ComovingVelocity::read);
        Ok(RedshiftPlan {
            measured: self.reading(&r.column, error.as_ref())?,
            kind,
            frame: r.frame,
            comoving: comoving
                .transpose()
                .map_err(|e| self.unsupported(e.to_string()))?,
        })
    }

    fn distance(&self, d: &DistanceReading) -> Result<DistancePlan, NormalizeError> {
        let how = match (&d.kind, &d.method, &d.by_code) {
            (Some(kind), Some(method), _) => ByCode::Fixed((*kind, method.clone())),
            (_, _, Some(codes)) => ByCode::Code {
                column: self.index(&codes.column)?,
                codes: codes
                    .codes
                    .iter()
                    .map(|(code, Coded { kind, method })| (code.clone(), (*kind, method.clone())))
                    .collect(),
            },
            _ => return Err(self.unsupported("a distance has no kind")),
        };
        let also_published = d
            .also_published
            .as_ref()
            .map(|p| self.reading(&p.value, p.error.as_ref()))
            .transpose()?;
        Ok(DistancePlan {
            measured: self.reading(&d.value, d.error.as_ref())?,
            also_published,
            how,
            measure: d.measure,
            kind_of_value: d.kind_of_value,
            hubble_constant: d.hubble_constant,
        })
    }

    fn rule(&self, r: &Rule) -> Result<RulePlan, NormalizeError> {
        let distances = &self.definition.normalization.distances;
        let action = match (&r.leave_out, &r.kind_of_value, &r.distance) {
            (Some(reason), _, _) => RuleAction::LeaveOut(*reason),
            (None, Some(k), _) => RuleAction::Kind(
                k.columns
                    .iter()
                    .map(|c| self.index(c))
                    .collect::<Result<_, _>>()?,
                k.kind,
            ),
            (None, None, Some(d)) => RuleAction::Distance(
                distances
                    .iter()
                    .position(|x| x.value == d.value)
                    .ok_or_else(|| {
                        self.unsupported(format!("no distance is read from {}", d.value))
                    })?,
                (d.kind, d.method.clone()),
            ),
            (None, None, None) => return Err(self.unsupported("a rule does nothing")),
        };
        let conditions = std::iter::once(&r.when)
            .chain(&r.and)
            .map(|c| Ok((self.index(&c.column)?, c.clone())))
            .collect::<Result<_, NormalizeError>>()?;
        Ok(RulePlan { conditions, action })
    }

    fn supergalactic(&self) -> Result<Option<[Col; 3]>, NormalizeError> {
        let sg = match (
            self.field(Field::SupergalacticX)?,
            self.field(Field::SupergalacticY)?,
            self.field(Field::SupergalacticZ)?,
        ) {
            (Some(x), Some(y), Some(z)) => [x, y, z],
            (None, None, None) => return Ok(None),
            _ => return Err(self.unsupported("it maps only some of SGX, SGY and SGZ")),
        };
        if sg.iter().any(|c| c.unit.dimension() != Dimension::Velocity) {
            return Err(self.unsupported("its supergalactic coordinates are not in velocity space"));
        }
        Ok(Some(sg))
    }
}

impl Plan {
    /// Resolves `definition` against a table whose columns are `header`.
    ///
    /// # Errors
    ///
    /// Fails when the header lacks a column the definition takes, the definition maps no
    /// identifier, or it asks for something the normalizer cannot do, such as a redshift
    /// without a position.
    pub fn new(definition: &SourceDefinition, header: &[&str]) -> Result<Self, NormalizeError> {
        let r = Resolver { definition, header };
        let identifier = r
            .field(Field::Identifier)?
            .ok_or_else(|| r.unsupported("its definition maps no identifier column"))?
            .index;
        let position = r.pair(Field::RightAscension, Field::Declination)?;
        if position.is_some() && !matches!(definition.frame, Frame::Icrs | Frame::Fk5J2000) {
            return Err(r.unsupported(format!(
                "its positions are in {:?}, not an equatorial frame",
                definition.frame
            )));
        }
        let n = &definition.normalization;
        if !n.redshifts.is_empty() && position.is_none() {
            return Err(
                r.unsupported("it reads a redshift but publishes no position to correct it with")
            );
        }
        if definition.fixes.len() > MAX_FIXES {
            return Err(r.unsupported(format!("it has more than {MAX_FIXES} fixes")));
        }
        let fixes: Vec<FixPlan> = definition
            .fixes
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.action.as_ref().map(|a| r.fix(i, a)))
            .collect::<Result<_, _>>()?;
        let leaving = fixes
            .iter()
            .filter(|f| {
                matches!(
                    f.effect,
                    FixEffect::ZeroLeavesOut(_) | FixEffect::LeaveOutWhen { .. }
                )
            })
            .fold(0, |bits, f| bits | 1 << f.fix);
        Ok(Self {
            identifier,
            name: r.field(Field::Name)?.map(|c| c.index),
            position,
            position_error: r.pair(Field::RightAscensionError, Field::DeclinationError)?,
            frame: definition.frame,
            velocity: r.measured(Field::Velocity, Field::VelocityError)?,
            parallax: r.measured(Field::Parallax, Field::ParallaxError)?,
            proper_motion: r.pair(Field::ProperMotionRa, Field::ProperMotionDec)?,
            magnitude: r.field(Field::Magnitude)?,
            group_id: r.field(Field::GroupId)?.map(|c| c.index),
            supergalactic: r.supergalactic()?,
            flags: definition
                .columns
                .iter()
                .filter(|c| c.field == Field::Quality)
                .map(|c| r.col(&c.column))
                .collect::<Result<_, _>>()?,
            redshifts: n
                .redshifts
                .iter()
                .map(|x| r.redshift(x))
                .collect::<Result<_, _>>()?,
            distances: n
                .distances
                .iter()
                .map(|x| r.distance(x))
                .collect::<Result<_, _>>()?,
            rules: n
                .rules
                .iter()
                .map(|x| r.rule(x))
                .collect::<Result<_, _>>()?,
            fixes,
            leaving,
            distance_unit: if definition.scale_domain == ScaleDomain::Stars {
                Unit::Parsec
            } else {
                Unit::Megaparsec
            },
        })
    }
}
