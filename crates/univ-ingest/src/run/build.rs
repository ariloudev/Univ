//! The build stage of a run: the cross-matched objects, the tiles, the researched structures,
//! the volume grids and the sources assembled into the next release, checked, and made complete
//! under the build output's `releases/{n}`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::{FetchOptions, RunReport, lock};
use crate::acquire::Acquired;
use crate::crossmatch::CrossMatched;
use crate::display::SourceState;
use crate::events::{Count, Event, EventSink, Stage};
use crate::release::{BuiltRelease, CONFIG_FILE, Inputs, ReleaseConfig, build, is_grid, read_grid};
use crate::source::{Access, SourceDefinition};
use crate::structures::read_all_named;
use crate::tiles::Built;

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

/// A size in decimal units, as the D1 cap is given.
fn decimal(bytes: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "a size shown to three figures")]
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.1} kB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        _ => format!("{:.2} GB", b / 1e9),
    }
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
            "D1: {} rows estimated at {} of the 10 GB cap ({share:.4}%)",
            m.d1.tables.iter().map(|t| t.rows).sum::<u64>(),
            decimal(m.d1.estimated_bytes)
        ),
        format!(
            "cell index: {} placed objects, {} of {} structures with a bounding box",
            m.counts.object_cells, m.counts.structures_indexed, m.counts.structures
        ),
    ];
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

/// Reads what the release holds beside the cross-match, the structures and grids, and builds
/// it. Refuses when a source failed.
fn assemble_and_build(
    definitions: &[SourceDefinition],
    acquired: &[Option<Acquired>],
    crossed: &CrossMatched,
    tiles: &Built,
    options: &FetchOptions,
    build_options: &BuildOptions,
) -> Result<BuiltRelease, String> {
    let rules = options.repository.join("rules");
    let config = ReleaseConfig::read(&rules.join(CONFIG_FILE)).map_err(|e| e.to_string())?;
    let mut held = Vec::new();
    let mut structures = Vec::new();
    let mut grids = Vec::new();
    for (definition, on_disk) in definitions.iter().zip(acquired) {
        let Some(on_disk) = on_disk else { continue };
        held.push(definition.clone());
        if let Access::Repository { path } = &definition.access {
            let dir = options.repository.join(path);
            structures.extend(read_all_named(&dir).map_err(|e| e.to_string())?);
        } else if is_grid(definition) {
            grids.push((definition.clone(), read_grid(definition, &on_disk.files)?));
        }
    }
    let inputs = Inputs {
        crossed,
        tiles,
        definitions: &held,
        structures: &structures,
        grids: &grids,
        config: &config,
        rules: &rules,
        cache: &options.cache,
        built_at: humantime::format_rfc3339_seconds(SystemTime::now()).to_string(),
    };
    build(&inputs, &build_options.state, &build_options.out).map_err(|e| e.to_string())
}

/// Builds the release of `crossed` and `tiles` under `build_options.out`, and takes the pending
/// state as the state directory's once it is complete. A run in which a source failed builds
/// nothing and names the sources. A stage that fails is reported in `report`, and gives
/// nothing.
#[expect(
    clippy::too_many_arguments,
    reason = "a stage takes the run's sink, sources, inputs, options and report, as the others do"
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
) -> Option<BuiltRelease> {
    lock(report).stage = Stage::Release;
    sink.emit(Event::StageStarted {
        stage: Stage::Release,
        items: None,
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
        tokio::task::spawn_blocking(move || {
            assemble_and_build(
                &definitions,
                &acquired,
                &crossed,
                &tiles,
                &options,
                &build_options,
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
            Some(built)
        }
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
