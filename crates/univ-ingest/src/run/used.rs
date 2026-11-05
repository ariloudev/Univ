//! What a run did with a source that is no table of rows, and so is not normalized: a volume
//! grid, read as one when a release is built, or a file of researched structures, which the
//! cross-match reads.

use std::sync::Mutex;

use super::{RunReport, lock};
use crate::acquire::Acquired;
use crate::display::SourceState;
use crate::normalize::reads_rows;
use crate::release::is_grid;
use crate::source::{Holds, SourceDefinition};

/// How far a run took the sources that are no table of rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reached {
    /// Normalized the tables, which these are not.
    Normalize,
    /// Read the structure files in the cross-match.
    Match,
    /// Built release n, which holds them.
    Built(u32),
}

/// What became of `definition`, which is no table of rows, once the run `reached` a stage.
fn how(definition: &SourceDefinition, reached: Reached) -> String {
    let grid = is_grid(definition);
    let structures = matches!(definition.access.repository(), Some((Holds::Structures, _)));
    match (grid, structures, reached) {
        (true, _, Reached::Built(n)) => format!("volume grid, read as a grid into release {n}"),
        (true, ..) => "volume grid, not normalized: read as a grid by a build".to_owned(),
        (_, true, Reached::Built(n)) => {
            format!("structure file, read by the cross-match into release {n}")
        }
        (_, true, Reached::Match) => "structure file, read by the cross-match".to_owned(),
        (_, true, Reached::Normalize) => {
            "structure file, not normalized: read by a cross-match".to_owned()
        }
        (false, false, _) => "no table of rows, so not normalized".to_owned(),
    }
}

/// Marks each source of `definitions` that the fetch left on disk and that is no table of rows
/// with what the run did with it, once it `reached` a stage.
pub fn mark(
    report: &Mutex<RunReport>,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    reached: Reached,
) {
    let mut report = lock(report);
    for (n, (definition, on_disk)) in definitions.iter().zip(acquired).enumerate() {
        if on_disk.is_some() && !reads_rows(definition) {
            report.set(n, SourceState::Used(how(definition, reached)));
        }
    }
}
