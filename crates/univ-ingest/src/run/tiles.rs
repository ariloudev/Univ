//! The tiles stage of a run: the cross-matched objects of each scale domain laid out as an
//! octree of tiles under the repository's tile rules, and written with a manifest per domain.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::{RunReport, lock};
use crate::crossmatch::CrossMatched;
use crate::events::{Count, Event, EventSink, Stage};
use crate::source::SourceDefinition;
use crate::tiles::{Built, Context, DOMAINS, build, name_of, write_all};

/// Where the tiles stage writes, by default: the build directory of the cache.
pub const DEFAULT_TILES: &str = ".cache/univ-ingest/build/tiles";

/// What a tiles stage is asked to do beyond a cross-match.
#[derive(Clone, Debug)]
pub struct TileOptions {
    /// The directory each scale domain's tiles and manifest are written under, or none to keep
    /// them for the release that follows.
    pub out: Option<PathBuf>,
}

/// A scale domain's name in a run's lines: `nearby galaxies` for `nearby_galaxies`.
fn spoken(name: &str) -> String {
    name.replace('_', " ")
}

/// What the stage counts: each domain's nodes, points and largest tile, and the objects in no
/// tile.
#[must_use]
pub fn counts(built: &Built) -> Vec<Count> {
    let mut counts = Vec::new();
    for d in &built.domains {
        let name = spoken(name_of(d.domain));
        counts.push(Count::new(d.nodes.len() as u64, format!("{name} nodes")));
        counts.push(Count::new(d.points() as u64, format!("{name} points")));
        counts.push(Count::new(
            d.largest_tile() as u64,
            format!("{name} largest tile bytes"),
        ));
    }
    counts.push(Count::new(
        built.unplaced as u64,
        "unplaced objects in no tile",
    ));
    counts
}

/// Builds the tiles of `crossed` under the tile rules and release cosmology the cross-match
/// worked under, and writes them under
/// `tile_options.out` where it names a directory. Nothing is written unless every domain builds. A stage that fails is
/// reported in `report`, and gives nothing.
pub async fn tiles_stage(
    sink: Arc<dyn EventSink>,
    definitions: &[SourceDefinition],
    crossed: Arc<CrossMatched>,
    tile_options: &TileOptions,
    report: &Mutex<RunReport>,
) -> Option<Built> {
    lock(report).stage = Stage::Tiles;
    sink.emit(Event::StageStarted {
        stage: Stage::Tiles,
        items: Some(DOMAINS.len() as u64),
    });
    let (definitions, out, events) = (definitions.to_vec(), tile_options.out.clone(), sink.clone());
    let outcome = tokio::task::spawn_blocking(move || {
        let context = Context {
            release: &crossed.release,
            cosmology: &crossed.cosmology,
            rules: &crossed.basis.tiles,
            definitions: &definitions,
        };
        let built = build(&crossed.objects, &context).map_err(|e| e.to_string())?;
        for d in &built.domains {
            events.emit(Event::Stepped {
                stage: Stage::Tiles,
                step: format!(
                    "{}: {} nodes, {} points, largest tile {} bytes",
                    spoken(name_of(d.domain)),
                    d.nodes.len(),
                    d.points(),
                    d.largest_tile()
                ),
            });
        }
        if let Some(out) = out {
            write_all(&out, &built).map_err(|e| format!("the tiles cannot be written: {e}"))?;
        }
        Ok::<_, String>(built)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    match outcome {
        Ok(built) => {
            sink.emit(Event::StageEnded {
                stage: Stage::Tiles,
                counts: counts(&built),
            });
            Some(built)
        }
        Err(error) => {
            lock(report).failure = Some(error);
            sink.emit(Event::StageEnded {
                stage: Stage::Tiles,
                counts: vec![],
            });
            None
        }
    }
}
