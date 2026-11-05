//! The build stage of a run: the cross-matched objects, the tiles, the researched structures,
//! the volume grids and the sources assembled into the next release, checked, and made complete
//! under the build output's `releases/{n}`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use univ_model::release::RELEASE_BUDGET_BYTES;

use super::{FetchOptions, RunReport, lock};
use crate::acquire::Acquired;
use crate::crossmatch::CrossMatched;
use crate::display::SourceState;
use crate::events::{Count, Event, EventSink, Stage};
use crate::release::{
    BuiltRelease, Inputs, PHASES, build_watched, decimal_bytes, is_grid, table_sizes,
};
use crate::source::SourceDefinition;
use crate::tiles::Built;
use crate::watch::Watch;

/// Where the build stage writes, by default: the build directory of the cache.
pub const DEFAULT_BUILD: &str = ".cache/univ-ingest/build";

/// What a build is asked to do beyond a cross-match.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// The state directory, whose pending state the release takes once complete.
    pub state: PathBuf,
    /// The build output: each release in `releases/{n}` under it.
    pub out: PathBuf,
}

/// The lines a complete release adds to the run.
fn lines(built: &BuiltRelease) -> Vec<String> {
    let m = &built.manifest;
    #[expect(clippy::cast_precision_loss, reason = "a share shown as a percentage")]
    let share = m.d1.estimated_bytes as f64 / m.d1.cap_bytes as f64 * 100.0;
    let mut lines = vec![
        format!(
            "release {}: every check passed; {} files listed with their SHA-256 in {}",
            built.number,
            m.files.len(),
            built.dir.join("manifest.yaml").display()
        ),
        format!(
            "D1: the catalogue database's {} rows estimated at {} of the {} cap ({share:.4}%), \
             within the {} budget of a release",
            m.d1.tables.iter().map(|t| t.rows).sum::<u64>(),
            decimal_bytes(m.d1.estimated_bytes),
            decimal_bytes(m.d1.cap_bytes),
            decimal_bytes(RELEASE_BUDGET_BYTES)
        ),
        format!("D1 by table: {}", table_sizes(&m.d1).join(", ")),
    ];
    for shard in &m.shards {
        let runs: Vec<String> = shard
            .cells
            .iter()
            .map(|r| format!("{} cells of {}", r.cells, r.domain))
            .collect();
        lines.push(format!(
            "D1: object shard {}: {} objects ({} not placed) in {}, rows estimated at {}",
            shard.index,
            shard.objects,
            shard.unplaced,
            if runs.is_empty() {
                "no cell".to_owned()
            } else {
                runs.join(" and ")
            },
            decimal_bytes(shard.d1.estimated_bytes)
        ));
    }
    lines.push(format!(
        "cell index: {} placed objects, {} of {} structures with a bounding box",
        m.counts.object_cells, m.counts.structures_indexed, m.counts.structures
    ));
    if !m.cell_index.not_indexed.is_empty() {
        let each: Vec<String> = m
            .cell_index
            .not_indexed
            .iter()
            .map(|n| format!("{} ({})", n.structure, n.reason))
            .collect();
        lines.push(format!(
            "cell index: {} structures not indexed: {}",
            each.len(),
            each.join("; ")
        ));
    }
    if !built.provisional.is_empty() {
        let ids: Vec<String> = built.provisional.iter().map(ToString::to_string).collect();
        lines.push(format!(
            "provisional redistribution review, judged again before any public deployment: {}",
            ids.join(", ")
        ));
    }
    let pending: Vec<String> = built
        .manifest
        .sources
        .iter()
        .filter_map(|s| {
            let asked = s.permission.as_ref()?;
            Some(format!(
                "{} (asked of {} on {})",
                s.id, asked.asked_of, asked.asked_on
            ))
        })
        .collect();
    if !pending.is_empty() {
        lines.push(format!(
            "permission to redistribute pending, published all the same: {}",
            pending.join("; ")
        ));
    }
    lines
}

/// What the stage counts.
fn counts(built: &BuiltRelease) -> Vec<Count> {
    let c = &built.manifest.counts;
    vec![
        Count::new(c.objects, "objects"),
        Count::new(c.placed, "placed"),
        Count::new(c.structures, "structures"),
        Count::new(c.grids, "volume grids"),
        Count::new(c.tiles, "tiles"),
        Count::new(built.manifest.files.len() as u64, "files"),
        Count::new(built.manifest.d1.estimated_bytes, "D1 bytes estimated"),
        Count::new(built.provisional.len() as u64, "provisional sources"),
    ]
}

/// Builds the release under the rules, configuration and structures the cross-match worked
/// under, with the volume grids among the fetched sources, each read as it is written.
fn assemble_and_build(
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    crossed: &CrossMatched,
    tiles: &Built,
    options: &FetchOptions,
    build_options: &BuildOptions,
    watch: Watch<'_>,
) -> Result<BuiltRelease, String> {
    let mut held = Vec::new();
    let mut grids = Vec::new();
    for (definition, on_disk) in definitions.iter().zip(acquired) {
        let Some(on_disk) = on_disk else { continue };
        held.push(definition.clone());
        if is_grid(definition) {
            grids.push((definition.clone(), on_disk.files.clone()));
        }
    }
    let inputs = Inputs {
        crossed,
        tiles,
        definitions: &held,
        grids: &grids,
        cache: &options.cache,
        built_at: super::now(),
    };
    build_watched(&inputs, &build_options.state, &build_options.out, watch)
        .map_err(|e| e.to_string())
}

/// Builds the release of `crossed` and `tiles` under `build_options.out`, and takes the pending
/// state as the state directory's once it is complete. A run in which a source failed builds
/// nothing and names the sources. A stage that fails is reported in `report`, and gives
/// nothing. Once `cancel` is set the build stops at its next safe point, leaving no release
/// directory and using up no number, and gives nothing.
#[expect(
    clippy::too_many_arguments,
    reason = "a stage takes the run's sink, sources, inputs, options, report and cancel flag"
)]
pub async fn build_stage(
    sink: Arc<dyn EventSink>,
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    crossed: Arc<CrossMatched>,
    tiles: Built,
    options: &FetchOptions,
    build_options: &BuildOptions,
    report: &Mutex<RunReport>,
    cancel: &Arc<AtomicBool>,
) -> Option<BuiltRelease> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    lock(report).stage = Stage::Release;
    sink.emit(Event::StageStarted {
        stage: Stage::Release,
        items: Some(PHASES),
    });
    let failed: Vec<String> = lock(report)
        .sources()
        .iter()
        .filter(|s| matches!(s.state, SourceState::Failed(_)))
        .map(|s| s.source.to_string())
        .collect();
    let outcome = if failed.is_empty() {
        let (definitions, acquired) = (definitions.to_vec(), acquired.to_vec());
        let (options, build_options) = (options.clone(), build_options.clone());
        let (events, flag) = (sink.clone(), cancel.clone());
        tokio::task::spawn_blocking(move || {
            let step = |step| {
                events.emit(Event::Stepped {
                    stage: Stage::Release,
                    step,
                });
            };
            let watch = Watch::new(&flag, &step);
            assemble_and_build(
                &definitions,
                &acquired,
                &crossed,
                &tiles,
                &options,
                &build_options,
                watch,
            )
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r)
    } else {
        Err(format!(
            "no release was built, since source(s) failed: {}",
            failed.join(", ")
        ))
    };
    match outcome {
        Ok(built) => {
            for step in lines(&built) {
                sink.emit(Event::Stepped {
                    stage: Stage::Release,
                    step,
                });
            }
            sink.emit(Event::StageEnded {
                stage: Stage::Release,
                counts: counts(&built),
            });
            lock(report).built = Some(built.number);
            let reached = super::Reached::Built(built.number);
            super::used::mark(report, definitions, acquired, reached);
            Some(built)
        }
        Err(_) if cancel.load(Ordering::Relaxed) => None,
        Err(error) => {
            lock(report).failure = Some(error);
            sink.emit(Event::StageEnded {
                stage: Stage::Release,
                counts: vec![],
            });
            None
        }
    }
}
