use super::*;

fn geometry(axes: [Axis; 3]) -> GridGeometry {
    GridGeometry {
        frame: Frame::Supergalactic,
        epoch: Epoch::J2000,
        unit: Unit::MegaparsecOverH,
        axes,
        shape: [2, 3, 4],
        cell: 10.0,
        origin: [-5.0, -10.0, -15.0],
        assumed_cosmology: None,
    }
}

fn field(samples: Samples, uncertainty: CellUncertainty, components: Components) -> Field {
    Field {
        column: "d".into(),
        field: "density".into(),
        unit: Unit::Dimensionless,
        kind: KindOfValue::Reconstructed,
        components,
        samples,
        uncertainty,
        labels: vec![],
    }
}

#[test]
fn the_axis_order_decides_which_sample_a_cell_reads() {
    let xyz = geometry([Axis::X, Axis::Y, Axis::Z]);
    assert_eq!(xyz.sample_index([1, 2, 3]), Some((3 + 2) * 4 + 3));
    assert_eq!(xyz.sample_index([2, 0, 0]), None);
    let zyx = GridGeometry {
        shape: [4, 3, 2],
        ..geometry([Axis::Z, Axis::Y, Axis::X])
    };
    assert_eq!(zyx.sample_index([1, 2, 3]), Some((3 * 3 + 2) * 2 + 1));
    assert_eq!(zyx.cells_along(Axis::X), 2);
}

#[test]
fn cells_and_positions_round_trip_about_the_origin() {
    let g = geometry([Axis::X, Axis::Y, Axis::Z]);
    assert_eq!(g.centre([1, 2, 3]), [5.0, 10.0, 15.0]);
    assert_eq!(g.cell_at([4.9, 14.9, 15.0]), Some([1, 2, 3]));
    assert_eq!(g.cell_at([-10.1, 0.0, 0.0]), None);
    assert_eq!(g.cell_at([15.0, 0.0, 0.0]), None);
    assert_eq!(g.corner(), [-10.0, -15.0, -20.0]);
}

#[test]
fn values_carry_their_uncertainty_and_kind_and_labels_their_integers() {
    let g = geometry([Axis::X, Axis::Y, Axis::Z]);
    let n = g.cells();
    let values: Vec<f64> = (0..3 * n)
        .map(|i| f64::from(u32::try_from(i).unwrap()))
        .collect();
    let sigma = vec![0.5; 3 * n];
    let column = "v_std".to_string();
    let vector = Components::Vector { axes: Axis::XYZ };
    let real = field(
        Samples::Real(values),
        CellUncertainty::StandardDeviation { column, sigma },
        vector,
    );
    let labels = field(
        Samples::Label((0..24).collect()),
        CellUncertainty::NoneReported,
        Components::Scalar,
    );
    let grid = Grid::new("s".into(), g, vec![real.clone(), labels.clone()]).unwrap();
    let (q, kind) = grid.value(&real, [0, 0, 1], 2).unwrap().unwrap();
    assert_eq!(q.value, f64::from(u32::try_from(2 * n + 1).unwrap()));
    assert_eq!(q.uncertainty, Uncertainty::Symmetric { sigma: 0.5 });
    assert_eq!(kind, KindOfValue::Reconstructed);
    assert_eq!(grid.value(&real, [0, 0, 1], 3).unwrap(), None);
    assert_eq!(grid.label(&labels, [1, 0, 2]), Some(14));
    assert_eq!(grid.value(&labels, [1, 0, 2], 0).unwrap(), None);
}

#[test]
fn malformed_grids_are_refused() {
    let g = geometry([Axis::X, Axis::X, Axis::Z]);
    assert!(matches!(g.check(), Err(GridError::Axes(_))));
    let degree = GridGeometry {
        unit: Unit::Degree,
        ..geometry(Axis::XYZ)
    };
    assert_eq!(degree.check(), Err(GridError::NotALength(Unit::Degree)));
    let short = field(
        Samples::Real(vec![0.0; 23]),
        CellUncertainty::NoneReported,
        Components::Scalar,
    );
    assert!(Grid::new("s".into(), geometry(Axis::XYZ), vec![short]).is_err());
    let sigma = CellUncertainty::StandardDeviation {
        column: "e".into(),
        sigma: vec![0.1; 2],
    };
    let short_sigma = field(Samples::Real(vec![0.0; 24]), sigma, Components::Scalar);
    assert!(Grid::new("s".into(), geometry(Axis::XYZ), vec![short_sigma]).is_err());
}

#[test]
fn a_grid_read_back_with_samples_its_cells_do_not_match_is_refused() {
    let scalar = |n| {
        field(
            Samples::Real(vec![0.0; n]),
            CellUncertainty::NoneReported,
            Components::Scalar,
        )
    };
    let grid = Grid::new("s".into(), geometry(Axis::XYZ), vec![scalar(24)]).unwrap();
    let yaml = serde_norway::to_string(&grid).unwrap();
    assert_eq!(serde_norway::from_str::<Grid>(&yaml).unwrap(), grid);
    let tampered = Grid {
        fields: vec![scalar(23)],
        ..grid.clone()
    };
    let yaml = serde_norway::to_string(&tampered).unwrap();
    assert!(serde_norway::from_str::<Grid>(&yaml).is_err(), "{yaml}");
    let huge = GridGeometry {
        shape: [1 << 30, 1 << 30, 1 << 30],
        ..geometry(Axis::XYZ)
    };
    assert_eq!(huge.check(), Err(GridError::Malformed));
}
