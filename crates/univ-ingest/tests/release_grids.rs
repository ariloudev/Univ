//! A release's volume grids (fetch-a-volume-grid), from crops of the real files
//! (`tests/fixtures/grids/`): each field's samples as little-endian 32-bit floats in the grid's
//! own layout, and a JSON header naming the release, the source, the frame, the axis order, the
//! shape, the origin and the cell, the cell in h⁻¹ Mpc rescaled to the release cosmology.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use serde::Deserialize;
use univ_ingest::grids::{Window, read_cf4_basins, read_cf4pp};
use univ_ingest::release::grid_files;
use univ_ingest::tiles::sha256;
use univ_model::derive::Release;
use univ_model::grid::{Grid, Samples};
use univ_model::release::keys;

#[derive(Deserialize)]
struct Sidecar {
    crop: Crop,
}

#[derive(Deserialize)]
struct Crop {
    start: [usize; 3],
}

fn window(sidecar: &str) -> Window {
    let text = std::fs::read_to_string(common::grids_dir().join(sidecar)).unwrap();
    let sidecar: Sidecar = serde_norway::from_str(&text).unwrap();
    Window::Cutout {
        start: sidecar.crop.start,
    }
}

fn f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// The files of `grid` in release 3, by path, and its header.
fn written(grid: &Grid, id: &str) -> (Vec<(String, Vec<u8>)>, serde_json::Value) {
    let definition = common::definition_of(id);
    let files = grid_files(3, grid, &definition, &Release::planck_2018()).unwrap();
    let header = files
        .files
        .iter()
        .find(|(p, _)| p == &format!("grids/{id}/header.json"))
        .map(|(_, b)| serde_json::from_slice(b).unwrap())
        .unwrap();
    let header_bytes = &files
        .files
        .iter()
        .find(|(p, _)| p == &format!("grids/{id}/header.json"))
        .unwrap()
        .1;
    assert_eq!(files.row.header_key, keys::file(&sha256(header_bytes)));
    (files.files, header)
}

#[test]
fn a_grid_is_written_as_its_header_and_float_samples_in_its_own_layout() {
    let grid = read_cf4pp(
        &common::grids_dir().join("cf4pp-crop.npz"),
        window("cf4pp-crop.yaml"),
    )
    .unwrap()
    .grid;
    let (files, header) = written(&grid, "cf4pp-density");
    assert_eq!(header["release"], 3);
    assert_eq!(header["source"]["id"], "cf4pp-density");
    assert!(
        header["source"]["citation"]
            .as_str()
            .unwrap()
            .contains("2025")
    );
    assert_eq!(header["unit"], "megaparsec_over_h");
    // Published on its source's own cells: neither resampled nor holding cells of no data.
    assert!(header["resampling"].is_null() && header["no_data"].is_null());
    assert!(header.get("resampling").is_some() && header.get("no_data").is_some());
    assert_eq!(header["shape"], serde_json::json!(grid.geometry.shape));
    assert_eq!(header["axes"], serde_json::json!(grid.geometry.axes));
    assert_eq!(header["frame"], serde_json::json!(grid.geometry.frame));
    // h⁻¹ Mpc to Mpc under H0 = 67.66: the cell divided by h = 0.6766.
    let cell = grid.geometry.cell;
    let rescaled = header["cell_rescaled"]["rescaled"]["value"]
        .as_f64()
        .unwrap();
    assert!(
        (rescaled - cell / 0.6766).abs() < 1e-9 * rescaled,
        "{rescaled}"
    );
    assert_eq!(header["cell_rescaled"]["rescaled"]["unit"], "megaparsec");

    assert_eq!(
        header["fields"].as_array().unwrap().len(),
        grid.fields.len()
    );
    for (i, field) in grid.fields.iter().enumerate() {
        let entry = &header["fields"][i];
        let path = format!("grids/cf4pp-density/field-{i}.f32");
        let bytes = &files.iter().find(|(p, _)| p == &path).unwrap().1;
        assert_eq!(entry["samples"], keys::file(&sha256(bytes)));
        assert_eq!(entry["kind"], "reconstructed");
        let Samples::Real(values) = &field.samples else {
            unreachable!("CF4++ is real")
        };
        let read = f32s(bytes);
        assert_eq!(read.len(), values.len());
        #[expect(clippy::cast_possible_truncation, reason = "the samples are f32")]
        let same = values
            .iter()
            .zip(&read)
            .all(|(v, r)| (*v as f32).to_bits() == r.to_bits());
        assert!(same, "{path}");
        assert_eq!(entry["uncertainty"]["form"], "standard_deviation");
    }
}

#[test]
fn a_label_grid_is_written_with_its_labels_exact() {
    let grid = read_cf4_basins(
        &common::grids_dir().join("cf4-basins-crop.zip"),
        window("cf4-basins-crop.yaml"),
    )
    .unwrap();
    let (files, header) = written(&grid, "cf4-basins");
    // Published in h⁻¹ Mpc for H0 = 74.6, which the header keeps beside the rescaled cell.
    assert_eq!(header["cell_rescaled"]["rescaled"]["unit"], "megaparsec");
    assert_eq!(header["assumed_cosmology"]["hubble_constant"], 74.6);
    let Samples::Label(labels) = &grid.fields[0].samples else {
        unreachable!("basins are labels")
    };
    let bytes = &files
        .iter()
        .find(|(p, _)| p == "grids/cf4-basins/field-0.f32")
        .unwrap()
        .1;
    #[expect(clippy::cast_precision_loss, reason = "labels below 2^24 are exact")]
    let exact = labels
        .iter()
        .zip(f32s(bytes))
        .all(|(l, r)| (*l as f32).to_bits() == r.to_bits());
    assert!(exact);
    assert_eq!(header["fields"][0]["uncertainty"]["form"], "none_reported");
}
