//! A row's ages.

use thin_vec::ThinVec;
use univ_model::entry::Age;

use super::{Row, fitted};

impl Row<'_, '_> {
    /// The ages the plan reads, each where its value is not blank, kept as published and
    /// normalized to years. An age that cannot stand, such as one whose credible interval does
    /// not hold it, is left out, and not the row: a cluster's place does not depend on its age.
    pub(super) fn ages(&mut self) -> ThinVec<Age> {
        let mut ages = ThinVec::with_capacity(self.cx.plan.ages.len());
        for plan in &self.cx.plan.ages {
            let Ok(Some(native)) = self.quantity(&plan.value) else {
                continue;
            };
            let kind = self.kind(plan.value.value.index, plan.kind);
            if let Ok(age) = Age::normalize(native, kind) {
                ages.push(age);
            }
        }
        fitted(ages)
    }
}
