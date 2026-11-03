//! The fixes of a definition resolved against a table's header: what each does to a row.

use std::collections::HashMap;

use univ_model::distance::{DistanceKind, Measure, Method};
use univ_model::units::Unit;

use super::super::NormalizeError;
use super::Resolver;
use crate::source::{Condition, FixAction, LeftOutReason};
use crate::table::identifier_key;

/// A fix the normalizer applies, by its index among the definition's fixes.
pub(in crate::normalize) struct FixPlan {
    pub fix: usize,
    pub columns: Vec<usize>,
    pub effect: FixEffect,
}

pub(in crate::normalize) enum FixEffect {
    /// The columns are read in another unit, already in each [`Col`].
    ReadAs,
    /// An error equal to this value, 0 for a zero error, was not reported.
    SentinelIsNoneReported(f64),
    ZeroLeavesOut(LeftOutReason),
    /// The columns are blank on rows whose cell in `column` meets `when`.
    DropWhen {
        column: usize,
        when: Condition,
    },
    /// The columns take the cited values on the row whose `key` cell, as compared, is a key of
    /// `values`; the `drop` columns are blank there.
    Replace {
        key: usize,
        values: HashMap<String, Vec<String>>,
        drop: Vec<usize>,
        /// The flag the entry carries, and its text: the citation and the fix.
        mark: (String, String),
    },
    /// A cited distance, value and symmetric error in `unit`, added on the row whose `key`
    /// cell, as compared, is a key of `values`, beside the source's own.
    AddDistance {
        key: usize,
        values: HashMap<String, (f64, f64)>,
        unit: Unit,
        how: (DistanceKind, Method),
        measure: Measure,
        /// The flag the entry carries, and its text: the citation and the fix.
        mark: (String, String),
    },
    /// The rows whose cell in `column` meets `when` are left out for `reason`.
    LeaveOutWhen {
        column: usize,
        when: Condition,
        reason: LeftOutReason,
    },
}

impl Resolver<'_> {
    pub(super) fn fix(&self, fix: usize, action: &FixAction) -> Result<FixPlan, NormalizeError> {
        let columns = action
            .columns()
            .iter()
            .map(|c| self.index(c))
            .collect::<Result<_, _>>()?;
        let effect = match action {
            FixAction::ReadAs { .. } => FixEffect::ReadAs,
            FixAction::ZeroErrorIsNoneReported { .. } => FixEffect::SentinelIsNoneReported(0.0),
            FixAction::SentinelErrorIsNoneReported { value, .. } => {
                FixEffect::SentinelIsNoneReported(*value)
            }
            FixAction::ZeroErrorLeavesOut { reason, .. } => FixEffect::ZeroLeavesOut(*reason),
            FixAction::DropWhen { when, .. } => FixEffect::DropWhen {
                column: self.index(&when.column)?,
                when: when.clone(),
            },
            FixAction::ReplaceByKey {
                key,
                drop,
                mark,
                values,
                ..
            } => FixEffect::Replace {
                mark: (mark.flag.clone(), self.cited(fix, &mark.cite)),
                key: self.index(key)?,
                values: values
                    .iter()
                    .map(|(k, v)| {
                        let key = identifier_key(k).to_owned();
                        (key, v.iter().map(f64::to_string).collect())
                    })
                    .collect(),
                drop: drop
                    .iter()
                    .map(|c| self.index(c))
                    .collect::<Result<_, _>>()?,
            },
            FixAction::AddDistance {
                key,
                unit,
                distance_kind,
                method,
                measure,
                mark,
                values,
            } => FixEffect::AddDistance {
                key: self.index(key)?,
                values: values
                    .iter()
                    .map(|(k, [v, e])| (identifier_key(k).to_owned(), (*v, *e)))
                    .collect(),
                unit: *unit,
                how: (*distance_kind, method.clone()),
                measure: *measure,
                mark: (mark.flag.clone(), self.cited(fix, &mark.cite)),
            },
            FixAction::LeaveOutWhen { when, reason } => FixEffect::LeaveOutWhen {
                column: self.index(&when.column)?,
                when: when.clone(),
                reason: *reason,
            },
        };
        Ok(FixPlan {
            fix,
            columns,
            effect,
        })
    }

    /// The text of a fix's flag: its citation and the fix that gave it.
    fn cited(&self, fix: usize, cite: &str) -> String {
        format!(
            "{cite} (fixes[{fix}] of the {} definition)",
            self.definition.id
        )
    }
}
