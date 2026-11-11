//! A row's ages.

use thin_vec::ThinVec;
use univ_model::entry::Age;

use super::{Row, fitted, value_reason};
use crate::source::LeftOutReason;

impl Row<'_, '_> {
    /// The ages the plan reads, each where its value is not blank, kept as published and
    /// normalized to years.
    pub(super) fn ages(&mut self) -> Result<ThinVec<Age>, LeftOutReason> {
        let mut ages = ThinVec::with_capacity(self.cx.plan.ages.len());
        for plan in &self.cx.plan.ages {
            let Some(native) = self.quantity(&plan.value)? else {
                continue;
            };
            let kind = self.kind(plan.value.value.index, plan.kind);
            ages.push(Age::normalize(native, kind).map_err(|e| value_reason(&e))?);
        }
        Ok(fitted(ages))
    }
}
