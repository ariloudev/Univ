//! The fixes a row applies before it is read: rows left out, values dropped or replaced with
//! cited ones, and the fixes marked that changed the row.

use univ_model::distance::Distance;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use super::super::plan::FixEffect;
use super::{Row, distance_reason};
use crate::source::LeftOutReason;
use crate::table::identifier_key;

impl Row<'_, '_> {
    /// Leaves the row out when a fix says a zero error in one of its columns marks it.
    pub(super) fn zero_errors(&mut self) -> Result<(), LeftOutReason> {
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
    pub(super) fn drop_values(&mut self) {
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
    pub(super) fn read_as(&mut self) {
        for fix in &self.cx.plan.fixes {
            if matches!(fix.effect, FixEffect::ReadAs)
                && fix.columns.iter().any(|c| !self.cell(*c).is_empty())
            {
                self.changed |= 1 << fix.fix;
            }
        }
    }

    /// Leaves the row out when it meets the condition of a fix that leaves such rows out.
    pub(super) fn leave_out_when(&mut self) -> Result<(), LeftOutReason> {
        for fix in &self.cx.plan.fixes {
            let FixEffect::LeaveOutWhen {
                column,
                when,
                reason,
            } = &fix.effect
            else {
                continue;
            };
            if when.matches(self.cell(*column)) {
                self.changed |= 1 << fix.fix;
                return Err(*reason);
            }
        }
        Ok(())
    }

    /// Replaces the cells of each fix that cites values for the row's key, and drops the
    /// columns the fix drops with them.
    pub(super) fn replace_values(&mut self) {
        let fixes = &self.cx.plan.fixes;
        for fix in fixes {
            let FixEffect::Replace {
                key,
                values,
                drop,
                mark,
            } = &fix.effect
            else {
                continue;
            };
            let Some(cited) = values.get(identifier_key(self.cell(*key))) else {
                continue;
            };
            self.changed |= 1 << fix.fix;
            self.marks.push((mark.0.as_str(), mark.1.as_str()));
            self.dropped.extend(drop);
            for (column, value) in fix.columns.iter().zip(cited) {
                self.replaced.push((*column, value.as_str()));
            }
        }
    }

    /// The cited distances fixes add to this row, beside its own, marking each fix and flag.
    pub(super) fn added_distances(&mut self) -> Result<Vec<Distance>, LeftOutReason> {
        let mut added = Vec::new();
        for fix in &self.cx.plan.fixes {
            let FixEffect::AddDistance {
                key,
                values,
                unit,
                how,
                measure,
                mark,
            } = &fix.effect
            else {
                continue;
            };
            let Some(&(value, sigma)) = values.get(identifier_key(self.cell(*key))) else {
                continue;
            };
            self.changed |= 1 << fix.fix;
            self.marks.push((mark.0.as_str(), mark.1.as_str()));
            let native = Quantity::new(value, *unit, Uncertainty::Symmetric { sigma })
                .map_err(|_| LeftOutReason::UnusableValue)?;
            let distance = Distance::new(
                native,
                None,
                self.cx.plan.distance_unit,
                KindOfValue::Measured,
                how.clone(),
                *measure,
                None,
            )
            .map_err(distance_reason)?;
            added.push(distance);
        }
        Ok(added)
    }
}
