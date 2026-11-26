//! The Galaxy model's definition file, `rules/galaxy-model.yaml` (build-the-galaxy-model).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use common::workspace;
use univ_ingest::galaxy_model::{ModelFile, Number};
use univ_model::galaxy_model::{Density, Light, ParameterSource};

fn file() -> std::path::PathBuf {
    workspace().join("rules/galaxy-model.yaml")
}

#[test]
fn the_definition_reads_and_asks_for_10_million_points() {
    let m = ModelFile::read(&file()).map_err(|e| e.to_string()).unwrap();
    assert_eq!(m.points, 10_000_000);
    let model = m.model().unwrap();
    assert_eq!(model.points, 10_000_000);
    assert!((model.r0.value - 8.15).abs() < 1e-12);
    assert!((model.sun_height.value - 0.0055).abs() < 1e-12, "in kpc");
}

#[test]
fn the_definition_gives_six_populations_each_with_its_density_height_light_and_colour() {
    let model = ModelFile::read(&file()).unwrap().model().unwrap();
    let ids: Vec<&str> = model.populations.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "thin_disc",
            "thick_disc",
            "bar_and_bulge",
            "stellar_halo",
            "young_stars",
            "star_forming_knots"
        ]
    );
    let codes: Vec<u8> = model.populations.iter().map(|p| p.code).collect();
    assert_eq!(codes, [1, 2, 3, 4, 5, 6]);
    for p in &model.populations {
        let height = match &p.density {
            Density::ExponentialDisc { scale_height, .. } | Density::Arms { scale_height, .. } => {
                scale_height.value
            }
            Density::Bar { bulge_scales, .. } => bulge_scales[2].value,
            Density::Halo { flattening, .. } => flattening.value,
        };
        assert!(height > 0.0, "{}", p.id);
        match &p.light {
            Light::Mass {
                mass,
                mass_to_light,
            } => assert!(mass.value > 0.0 && mass_to_light.value > 0.0, "{}", p.id),
            Light::Share { share } => assert!(share.value > 0.0, "{}", p.id),
        }
        // Each colour is derived from a colour index, or chosen with its reason.
        match &p.colour.srgb_from {
            ParameterSource::Derived { how } => {
                assert!(p.colour.b_minus_v.is_some() && !how.is_empty(), "{}", p.id);
            }
            ParameterSource::Choice { reason } => assert!(!reason.is_empty(), "{}", p.id),
            ParameterSource::Cited { .. } => panic!("{}: a colour is derived or chosen", p.id),
        }
    }
}

#[test]
fn each_number_of_the_definition_cites_its_paper_or_is_a_choice_with_its_reason() {
    let text = std::fs::read_to_string(file()).unwrap();
    let m: ModelFile = serde_norway::from_str(&text).unwrap();
    let model = m.model().unwrap();
    let json = serde_json::to_value(&model).unwrap();
    let mut seen = 0;
    let mut stack = vec![&json];
    while let Some(v) = stack.pop() {
        if let Some(source) = v.get("source") {
            seen += 1;
            match source["from"].as_str() {
                Some("cited") => {
                    assert!(!source["paper"].as_str().unwrap().is_empty(), "{v}");
                    assert!(!source["locator"].as_str().unwrap().is_empty(), "{v}");
                }
                Some("choice") => assert!(!source["reason"].as_str().unwrap().is_empty(), "{v}"),
                other => panic!("{other:?}: {v}"),
            }
        }
        match v {
            serde_json::Value::Object(o) => stack.extend(o.values()),
            serde_json::Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    assert!(seen > 40, "{seen}");
    assert!(matches!(m.r0, Number::Cited(_)));
}

/// The repository's definition, `edit` applied to it, read.
fn edited(edit: impl Fn(String) -> String) -> Result<ModelFile, String> {
    let text = std::fs::read_to_string(file()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("galaxy-model.yaml");
    std::fs::write(&path, edit(text)).unwrap();
    ModelFile::read(&path).map_err(|e| e.to_string())
}

#[test]
fn a_number_that_neither_cites_a_paper_nor_is_a_choice_is_refused_naming_it() {
    assert!(edited(|t| t).is_ok());
    // The thin disc's edge, a choice, with its choice taken away.
    let unmarked = edited(|t| {
        let at = t.find("max_radius:").unwrap();
        let choice = at + t[at..].find("choice:").unwrap();
        let line = t[..choice].rfind('\n').unwrap();
        let end = choice + t[choice..].find("follows_arms").unwrap();
        let end = t[..end].rfind('\n').unwrap();
        format!("{}{}", &t[..line], &t[end..])
    })
    .unwrap_err();
    assert!(unmarked.contains("galaxy-model.yaml"), "{unmarked}");
    assert!(unmarked.contains("max_radius"), "{unmarked}");
    assert!(unmarked.contains("the number 25"), "{unmarked}");
    assert!(unmarked.contains("this project's choice"), "{unmarked}");
    let reasonless = edited(|t| {
        t.replacen(
            "choice: \"As the thin disc's: beyond the farthest of the arms' fits.\"",
            "choice: \" \"",
            1,
        )
    })
    .unwrap_err();
    assert!(reasonless.contains("gives no reason"), "{reasonless}");
}

#[test]
fn a_number_the_model_cannot_be_drawn_with_or_a_seed_beyond_2_53_is_refused() {
    let negative = edited(|t| {
        t.replacen(
            "    value: 2\n    unit: kiloparsec",
            "    value: -2\n    unit: kiloparsec",
            1,
        )
    })
    .unwrap_err();
    assert!(negative.contains("arms.same_stretch"), "{negative}");
    assert!(negative.contains("must be a positive number"), "{negative}");
    let heavy = edited(|t| t.replacen("    value: 0.45\n", "    value: 1.45\n", 1)).unwrap_err();
    assert!(heavy.contains("arms.extrapolation_weight"), "{heavy}");
    assert!(heavy.contains("at most 1"), "{heavy}");
    let seed = edited(|t| t.replacen("seed: 16\n", "seed: 9007199254740992\n", 1)).unwrap_err();
    assert!(seed.contains("seed") && seed.contains("2^53 - 1"), "{seed}");
}
