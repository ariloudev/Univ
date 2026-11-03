//! One row of a table read into a catalogue entry, or left out with its reason.

use std::borrow::Cow;
use std::collections::BTreeMap;

use univ_model::cosmology::AssumedCosmology;
use univ_model::distance::{Distance, DistanceError, DistanceKind, Method};
use univ_model::entry::{
    CatalogueEntry, EntryError, Magnitude, NativePosition, Position, PositionErrors,
    PositionUncertainty, ProperMotion, Redshift, RedshiftKind, SupergalacticVelocity,
};
use univ_model::ids::{Designation, SourceId};
use univ_model::position::{Epoch, PositionError};
use univ_model::redshift::CmbCorrection;
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value, ValueError};

pub(super) use super::plan::Changed;
use super::plan::{ByCode, Col, Errors, FixEffect, Measured, Plan, RuleAction};
use crate::source::LeftOutReason;

/// What the rows of one source share.
pub(super) struct Context<'a> {
    pub plan: &'a Plan,
    pub source: &'a SourceId,
    pub epoch: Epoch,
    pub correction: &'a CmbCorrection,
}

/// A row being read: its cells, the kinds of value its rules set, and the fixes that changed
/// it so far.
struct Row<'a, 'c> {
    cx: &'a Context<'a>,
    cells: &'a [Cow<'c, str>],
    kinds: Vec<(usize, KindOfValue)>,
    /// The kind and method a rule gives a distance, by its index among the plan's distances.
    methods: Vec<(usize, (DistanceKind, Method))>,
    /// Columns a fix dropped on this row, read as blank.
    dropped: Vec<usize>,
    changed: Changed,
}

/// Reads `cells` into an entry, or gives the reason it is left out. Either way it gives the
/// fixes that changed the row.
pub(super) fn read(
    cx: &Context<'_>,
    cells: &[Cow<'_, str>],
) -> Result<(CatalogueEntry, Changed), (LeftOutReason, Changed)> {
    let mut row = Row {
        cx,
        cells,
        kinds: Vec::new(),
        methods: Vec::new(),
        dropped: Vec::new(),
        changed: 0,
    };
    match row.entry() {
        Ok(entry) => Ok((entry, row.changed)),
        // A row left out was changed only by a fix that left it out.
        Err(reason) => Err((reason, row.changed & cx.plan.leaving)),
    }
}

impl Row<'_, '_> {
    fn cell(&self, index: usize) -> &str {
        if self.dropped.contains(&index) {
            return "";
        }
        self.cells.get(index).map_or("", |c| c.as_ref())
    }

    fn text(&self, index: usize) -> Option<String> {
        let text = self.cell(index);
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// What a reading gives this row: its one value, or the value its code column maps the
    /// row's code to.
    fn by_code<T: Clone>(&self, how: &ByCode<T>) -> Result<T, LeftOutReason> {
        match how {
            ByCode::Fixed(value) => Ok(value.clone()),
            ByCode::Code { column, codes } => codes
                .get(self.cell(*column))
                .cloned()
                .ok_or(LeftOutReason::UnknownMethodCode),
        }
    }

    /// The number in a cell: none when blank.
    fn number(&self, index: usize) -> Result<Option<f64>, LeftOutReason> {
        let cell = self.cell(index);
        if cell.is_empty() {
            return Ok(None);
        }
        match cell.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(Some(x)),
            _ => Err(LeftOutReason::UnparsableNumber),
        }
    }

    fn kind(&self, column: usize, default: KindOfValue) -> KindOfValue {
        self.kinds
            .iter()
            .find(|(c, _)| *c == column)
            .map_or(default, |(_, k)| *k)
    }

    fn mark(&mut self, effect: impl Fn(&FixEffect) -> bool, column: usize) -> bool {
        let fix = self
            .cx
            .plan
            .fixes
            .iter()
            .find(|f| effect(&f.effect) && f.columns.contains(&column));
        if let Some(f) = fix {
            self.changed |= 1 << f.fix;
        }
        fix.is_some()
    }

    fn entry(&mut self) -> Result<CatalogueEntry, LeftOutReason> {
        let plan = self.cx.plan;
        self.rules()?;
        self.zero_errors()?;
        self.drop_values();
        let identifier = self
            .text(plan.identifier)
            .ok_or(LeftOutReason::MissingIdentifier)?;
        let position = self.position()?;
        let direction = position.as_ref().map(|p| p.icrs.unit_vector());
        let mut redshifts = Vec::new();
        for r in &plan.redshifts {
            let Some(measured) = self.quantity(&r.measured)? else {
                continue;
            };
            let kind = self.by_code(&r.kind)?;
            let default = match kind {
                RedshiftKind::Spectroscopic => KindOfValue::Measured,
                RedshiftKind::Photometric => KindOfValue::Estimated,
            };
            let kind_of_value = self.kind(r.measured.value.index, default);
            let direction = direction.ok_or(LeftOutReason::MissingPosition)?;
            let redshift = Redshift::heliocentric(
                measured,
                kind,
                kind_of_value,
                direction,
                self.cx.correction,
            )
            .map_err(entry_reason)?;
            redshifts.push(redshift);
        }
        let mut distances = Vec::new();
        for (i, d) in plan.distances.iter().enumerate() {
            let Some(native) = self.quantity(&d.measured)? else {
                continue;
            };
            let how = match self.methods.iter().find(|(d, _)| *d == i) {
                Some((_, how)) => how.clone(),
                None => self.by_code(&d.how)?,
            };
            let also = match &d.also_published {
                Some(m) => self.quantity(m)?,
                None => None,
            };
            let kind_of_value = self.kind(d.measured.value.index, d.kind_of_value);
            let distance = Distance::new(
                native,
                also,
                plan.distance_unit,
                kind_of_value,
                how,
                d.measure,
                d.hubble_constant.map(AssumedCosmology::hubble),
            )
            .map_err(distance_reason)?;
            distances.push(distance);
        }
        let entry = CatalogueEntry {
            designation: Designation::new(self.cx.source.clone(), identifier)
                .map_err(|_| LeftOutReason::MissingIdentifier)?,
            name: plan.name.and_then(|c| self.text(c)),
            position,
            distances,
            redshifts,
            velocity: self.value(plan.velocity.as_ref(), Unit::KilometrePerSecond)?,
            parallax: self.value(plan.parallax.as_ref(), Unit::Degree)?,
            proper_motion: self.proper_motion()?,
            magnitude: self.magnitude()?,
            group_id: plan.group_id.and_then(|c| self.text(c)),
            supergalactic_velocity: self.supergalactic()?,
            flags: plan
                .flags
                .iter()
                .filter_map(|c| self.text(c.index).map(|t| (c.name.clone(), t)))
                .collect::<BTreeMap<_, _>>(),
        };
        entry.check().map_err(entry_reason)?;
        self.read_as();
        Ok(entry)
    }

    /// Applies the rules: a rule that leaves the row out ends it, and the others set kinds of
    /// value.
    fn rules(&mut self) -> Result<(), LeftOutReason> {
        for rule in &self.cx.plan.rules {
            if !rule
                .conditions
                .iter()
                .all(|(c, when)| when.matches(self.cell(*c)))
            {
                continue;
            }
            match &rule.action {
                RuleAction::LeaveOut(reason) => return Err(*reason),
                RuleAction::Kind(columns, kind) => {
                    self.kinds.extend(columns.iter().map(|c| (*c, *kind)));
                }
                RuleAction::Distance(d, how) => self.methods.push((*d, how.clone())),
            }
        }
        Ok(())
    }

    /// Leaves the row out when a fix says a zero error in one of its columns marks it.
    fn zero_errors(&mut self) -> Result<(), LeftOutReason> {
        for fix in &self.cx.plan.fixes {
            let FixEffect::ZeroLeavesOut(reason) = fix.effect else {
                continue;
            };
            for column in &fix.columns {
                if self.number(*column)? == Some(0.0) {
                    self.changed |= 1 << fix.fix;
                    return Err(reason);
                }
            }
        }
        Ok(())
    }

    /// Drops the values of the columns of each fix whose condition the row meets, marking the
    /// fix when it dropped a value that was there.
    fn drop_values(&mut self) {
        for fix in &self.cx.plan.fixes {
            let FixEffect::DropWhen { column, when } = &fix.effect else {
                continue;
            };
            if !when.matches(self.cell(*column)) {
                continue;
            }
            if fix.columns.iter().any(|c| !self.cell(*c).trim().is_empty()) {
                self.changed |= 1 << fix.fix;
            }
            self.dropped.extend(&fix.columns);
        }
    }

    /// Marks each fix that reads columns in another unit, for an entry that took a value from
    /// one of them.
    fn read_as(&mut self) {
        for fix in &self.cx.plan.fixes {
            if matches!(fix.effect, FixEffect::ReadAs)
                && fix.columns.iter().any(|c| !self.cell(*c).is_empty())
            {
                self.changed |= 1 << fix.fix;
            }
        }
    }

    fn position(&mut self) -> Result<Option<Position>, LeftOutReason> {
        let plan = self.cx.plan;
        let Some((ra, dec)) = &plan.position else {
            return Ok(None);
        };
        let (Some(lon), Some(lat)) = (self.number(ra.index)?, self.number(dec.index)?) else {
            return Err(LeftOutReason::MissingPosition);
        };
        let uncertainty = match &plan.position_error {
            Some((e_ra, e_dec)) => match (self.error_value(e_ra)?, self.error_value(e_dec)?) {
                (Some(ra), Some(dec)) => {
                    PositionUncertainty::Published(Box::new(PositionErrors { ra, dec }))
                }
                _ => PositionUncertainty::NoneReported,
            },
            None => PositionUncertainty::NoneReported,
        };
        let native = NativePosition {
            frame: plan.frame,
            lon,
            lat,
            unit: ra.unit,
            epoch: self.cx.epoch,
        };
        Position::new(native, uncertainty)
            .map(Some)
            .map_err(entry_reason)
    }

    /// An error published as a value of its own, such as a position error, in degrees.
    fn error_value(&mut self, col: &Col) -> Result<Option<Value>, LeftOutReason> {
        let Some(sigma) = self.number(col.index)? else {
            return Ok(None);
        };
        if sigma < 0.0 {
            return Err(LeftOutReason::BadUncertainty);
        }
        let native = Quantity::new(sigma, col.unit, Uncertainty::NoneReported)
            .map_err(|e| value_reason(&e))?;
        Value::normalize(native, Unit::Degree, KindOfValue::Measured)
            .map(Some)
            .map_err(|e| value_reason(&e))
    }

    /// The quantity a value column and its errors give, in the value's unit: none when the
    /// value is blank, and none reported when an error is blank or missing.
    fn quantity(&mut self, m: &Measured) -> Result<Option<Quantity>, LeftOutReason> {
        let Some(value) = self.number(m.value.index)? else {
            return Ok(None);
        };
        let unit = m.value.unit;
        let uncertainty = match &m.error {
            None => Uncertainty::NoneReported,
            Some(Errors::Symmetric(e)) => match self.error(e, unit)? {
                Some(sigma) => Uncertainty::Symmetric { sigma },
                None => Uncertainty::NoneReported,
            },
            Some(Errors::Asymmetric { minus, plus }) => {
                match (self.error(minus, unit)?, self.error(plus, unit)?) {
                    (Some(minus), Some(plus)) => Uncertainty::Asymmetric { minus, plus },
                    _ => Uncertainty::NoneReported,
                }
            }
            Some(Errors::CredibleInterval {
                low,
                high,
                probability,
            }) => match (self.bound(low, unit)?, self.bound(high, unit)?) {
                (Some(low), Some(high)) => Uncertainty::CredibleInterval {
                    minus: value - low,
                    plus: high - value,
                    probability: *probability,
                },
                _ => Uncertainty::NoneReported,
            },
        };
        Quantity::new(value, unit, uncertainty)
            .map(Some)
            .map_err(|e| value_reason(&e))
    }

    /// An error in `unit`: none when blank, or when zero and a fix says a zero error was not
    /// reported.
    fn error(&mut self, col: &Col, unit: Unit) -> Result<Option<f64>, LeftOutReason> {
        let Some(sigma) = self.number(col.index)? else {
            return Ok(None);
        };
        if sigma == 0.0 && self.mark(|e| matches!(e, FixEffect::ZeroIsNoneReported), col.index) {
            return Ok(None);
        }
        if sigma < 0.0 {
            return Err(LeftOutReason::BadUncertainty);
        }
        let factor = col
            .unit
            .factor_to(unit)
            .map_err(|_| LeftOutReason::UnusableValue)?;
        Ok(Some(sigma * factor))
    }

    /// A bound of a credible interval in `unit`.
    fn bound(&self, col: &Col, unit: Unit) -> Result<Option<f64>, LeftOutReason> {
        let factor = col
            .unit
            .factor_to(unit)
            .map_err(|_| LeftOutReason::UnusableValue)?;
        Ok(self.number(col.index)?.map(|b| b * factor))
    }

    fn value(&mut self, m: Option<&Measured>, to: Unit) -> Result<Option<Value>, LeftOutReason> {
        let Some(m) = m else { return Ok(None) };
        let Some(native) = self.quantity(m)? else {
            return Ok(None);
        };
        let kind = self.kind(m.value.index, KindOfValue::Measured);
        Value::normalize(native, to, kind)
            .map(Some)
            .map_err(|e| value_reason(&e))
    }

    fn plain(&mut self, col: &Col, to: Unit) -> Result<Option<Value>, LeftOutReason> {
        let m = Measured {
            value: col.clone(),
            error: None,
        };
        self.value(Some(&m), to)
    }

    fn proper_motion(&mut self) -> Result<Option<ProperMotion>, LeftOutReason> {
        let Some((ra, dec)) = &self.cx.plan.proper_motion else {
            return Ok(None);
        };
        let to = Unit::MilliarcsecondPerYear;
        Ok(self
            .plain(ra, to)?
            .zip(self.plain(dec, to)?)
            .map(|(ra, dec)| ProperMotion { ra, dec }))
    }

    fn magnitude(&mut self) -> Result<Option<Magnitude>, LeftOutReason> {
        let Some(col) = &self.cx.plan.magnitude else {
            return Ok(None);
        };
        Ok(self.plain(col, Unit::Magnitude)?.map(|value| Magnitude {
            band: col.name.clone(),
            value,
        }))
    }

    fn supergalactic(&mut self) -> Result<Option<SupergalacticVelocity>, LeftOutReason> {
        let Some([x, y, z]) = &self.cx.plan.supergalactic else {
            return Ok(None);
        };
        let to = Unit::KilometrePerSecond;
        let (x, y, z) = (self.plain(x, to)?, self.plain(y, to)?, self.plain(z, to)?);
        Ok(match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(SupergalacticVelocity { x, y, z }),
            _ => None,
        })
    }
}

fn value_reason(error: &ValueError) -> LeftOutReason {
    match error {
        ValueError::BadUncertainty(_) | ValueError::BadProbability(_) => {
            LeftOutReason::BadUncertainty
        }
        ValueError::NotFinite(_) | ValueError::Unit(_) => LeftOutReason::UnusableValue,
    }
}

fn distance_reason(error: DistanceError) -> LeftOutReason {
    match error {
        DistanceError::Value(e) => value_reason(&e),
        _ => LeftOutReason::UnusableValue,
    }
}

fn entry_reason(error: EntryError) -> LeftOutReason {
    match error {
        EntryError::Position(PositionError::RightAscension(_)) => {
            LeftOutReason::RightAscensionOutOfRange
        }
        EntryError::Position(PositionError::Declination(_)) => LeftOutReason::DeclinationOutOfRange,
        EntryError::Value(e) => value_reason(&e),
        _ => LeftOutReason::UnusableValue,
    }
}
