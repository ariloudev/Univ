//! The test of each structure outlined from a volume grid (RQ-23), run while the build holds the
//! grid ([`crate::outlines::place_on`]), against the entries of the test's catalogues and the
//! dense places the release holds, and recorded in `structure_details.outline`.
//!
//! A structure whose grid is not among the release's grids is recorded as refused, its outline
//! not drawn, and is shown at its published position as every refused outline is. The release
//! places an outlined structure at its published position either way; the record says whether
//! its test admitted the outline.

use std::collections::HashMap;

use serde_json::{Value as Json, json};
use univ_model::entry::CatalogueEntry;
use univ_model::grid::Grid;
use univ_model::ids::SourceId;

use crate::crossmatch::ReleaseObject;
use crate::outlines::{GridSky, OutlineResult, Placed, dense_places, place_on};
use crate::structures::{GridOutline, Structure};

/// The `outline` column of a structure whose outline was drawn: the grid, field, threshold and
/// test as recorded, the verdict and its reason, the region's size and the checks.
#[must_use]
pub fn result_json(r: &OutlineResult) -> Json {
    let (verdict, reason) = match &r.placed {
        Placed::Outline { .. } => ("admitted", None),
        Placed::Point { reason, .. } => ("refused", Some(reason.as_str())),
    };
    json!({
        "grid": r.grid,
        "field": r.field,
        "threshold": r.threshold,
        "test": r.test,
        "verdict": verdict,
        "reason": reason,
        "cells": r.cells,
        "touches_edge": r.touches_grid_edge,
        "checks": r.catalogues,
        "dense": r.dense,
    })
}

/// The `outline` column of a structure whose outline could not be drawn, with why.
#[must_use]
pub fn not_drawn(record: &GridOutline, reason: &str) -> Json {
    json!({
        "grid": record.grid,
        "field": record.field,
        "threshold": record.threshold,
        "test": record.test,
        "verdict": "refused",
        "reason": format!(
            "the outline was not drawn: {reason}; the structure is shown at its published position"
        ),
        "cells": 0,
        "touches_edge": false,
        "checks": [],
        "dense": null,
    })
}

/// Why the outline of a structure on `grid` is not drawn when the release holds no such grid.
#[must_use]
pub fn grid_missing(record: &GridOutline) -> String {
    format!(
        "its volume grid {} is not among this release's grids",
        record.grid
    )
}

/// Outlines each of `structures` recorded on `grid` and tests it against the entries of
/// `objects`, giving each structure's id and its `outline` column.
#[must_use]
pub fn outlines_on(
    grid: &Grid,
    structures: &[(String, Structure)],
    objects: &[ReleaseObject],
) -> Vec<(String, Json)> {
    let outlined: Vec<(&String, &Structure, &GridOutline)> = structures
        .iter()
        .filter_map(|(id, s)| {
            let record = s.outline.as_ref()?;
            (record.grid.as_str() == grid.source).then_some((id, s, record))
        })
        .collect();
    if outlined.is_empty() {
        return Vec::new();
    }
    let sky = GridSky::new(&grid.geometry);
    let wanted: Vec<&SourceId> = outlined
        .iter()
        .flat_map(|(_, _, r)| &r.test.catalogues)
        .collect();
    let mut entries: HashMap<&SourceId, Vec<&CatalogueEntry>> = HashMap::new();
    for e in objects.iter().flat_map(|o| &o.object.entries) {
        let source = &e.entry.designation.source;
        if wanted.contains(&source) {
            entries.entry(source).or_default().push(&e.entry);
        }
    }
    let plain: Vec<Structure> = structures.iter().map(|(_, s)| s.clone()).collect();
    outlined
        .into_iter()
        .map(|(id, s, record)| {
            // A catalogue the release holds no entry of is not given to the test.
            let catalogues: Vec<(SourceId, &[&CatalogueEntry])> = record
                .test
                .catalogues
                .iter()
                .filter_map(|c| Some((c.clone(), entries.get(c)?.as_slice())))
                .collect();
            let dense = record
                .test
                .dense_places
                .as_ref()
                .map(|rule| dense_places(rule, objects.iter().map(|o| &o.object), &plain))
                .unwrap_or_default();
            let column = match place_on(s, grid, &sky, &catalogues, &dense) {
                Ok(result) => result_json(&result),
                Err(e) => not_drawn(record, &e.to_string()),
            };
            (id.clone(), column)
        })
        .collect()
}
