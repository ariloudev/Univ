//! The Galaxy model of a release (read-the-galaxy-model), read from the test kit's release 2.

use serde_json::{Value, json};
use univ_model::galaxy_model::{
    ArmOrder, ArmRules, Colour, Density, Light, Model, ModelPaper, Parameter, ParameterSource,
    Population,
};
use univ_model::value::Uncertainty;

use crate::testkit::{RELEASES, STRUCTURES, Sqlite, body};

/// A model of one population, its scale length cited with its uncertainty and its edge chosen.
fn model() -> Model {
    let mut scale_length = Parameter::cited(2.6, "kiloparsec", "bhg-2016", "Table 1");
    scale_length.uncertainty = Uncertainty::Symmetric { sigma: 0.5 };
    Model {
        points: 20_000,
        seed: 16,
        r0: Parameter::cited(8.15, "kiloparsec", "reid-2019", "Table 3"),
        sun_height: Parameter::cited(0.0055, "kiloparsec", "reid-2019", "Section 4"),
        populations: vec![Population {
            code: 1,
            id: "thin_disc".to_owned(),
            name: "Thin disc".to_owned(),
            density: Density::ExponentialDisc {
                scale_length,
                scale_height: Parameter::cited(0.3, "kiloparsec", "bhg-2016", "Table 1"),
                max_radius: Parameter::choice(25.0, "kiloparsec", "Beyond the farthest arm."),
                follows_arms: true,
            },
            light: Light::Share {
                share: Parameter::choice(1.0, "1", "The only population."),
            },
            colour: Colour {
                b_minus_v: None,
                srgb: [255, 242, 237],
                srgb_from: ParameterSource::Choice {
                    reason: "A test colour.".to_owned(),
                },
            },
        }],
        arms: ArmRules {
            order: vec![ArmOrder {
                arm: "perseus".to_owned(),
                structures: vec!["reid-2019-perseus-arm".to_owned()],
            }],
            extrapolation_weight: Parameter::choice(0.45, "1", "Fainter."),
            same_stretch: Parameter::choice(2.0, "kiloparsec", "One stretch."),
            old_arm_contrast: Parameter::choice(1.3, "1", "A rise."),
            old_arm_spread: Parameter::choice(2.5, "1", "A spread."),
        },
        papers: vec![ModelPaper {
            id: "reid-2019".to_owned(),
            reference: "Reid et al. 2019, ApJ 885, 131".to_owned(),
            doi: Some("10.3847/1538-4357/ab4a11".to_owned()),
        }],
    }
}

/// Release 2 with its Galaxy model.
fn db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(STRUCTURES);
    let definition = serde_json::to_string(&model()).unwrap();
    db.exec(&format!(
        "INSERT INTO galaxy_models VALUES (2, 'galaxy_model', 20000, 16, '{}', \
         '[\"reid-2019-perseus-arm\"]', NULL);",
        definition.replace('\'', "''")
    ));
    db
}

fn read(db: &Sqlite, query: &str) -> Value {
    let response = db.get("/v1/galaxy-model", query);
    assert_eq!(response.status, 200, "{}", body(&response));
    body(&response)
}

#[test]
fn the_model_gives_its_points_seed_domain_and_the_arms_it_followed() {
    let m = read(&db(), "");
    assert_eq!(m["release"], 2);
    assert_eq!(m["domain"], "galaxy_model");
    assert_eq!(m["definition"]["points"], 20_000);
    assert_eq!(m["definition"]["seed"], 16);
    assert!(m.get("points").is_none(), "given once, in the definition");
    assert_eq!(m["followed"], json!(["reid-2019-perseus-arm"]));
}

#[test]
fn each_population_gives_its_code_colour_and_parameters_each_cited_with_its_uncertainty() {
    let m = read(&db(), "release=2");
    let disc = &m["definition"]["populations"][0];
    assert_eq!(disc["code"], 1);
    assert_eq!(disc["colour"]["srgb"], json!([255, 242, 237]));
    assert_eq!(disc["density"]["form"], "exponential_disc");
    let length = &disc["density"]["scale_length"];
    assert_eq!(length["value"], 2.6);
    assert_eq!(length["unit"], "kiloparsec");
    assert_eq!(length["kind"], "estimated");
    assert_eq!(
        length["uncertainty"],
        json!({"form": "symmetric", "sigma": 0.5})
    );
    assert_eq!(length["source"]["from"], "cited");
    assert_eq!(length["source"]["paper"], "bhg-2016");
    assert_eq!(length["source"]["locator"], "Table 1");
    assert_eq!(m["definition"]["papers"][0]["id"], "reid-2019");
}

#[test]
fn each_choice_gives_its_reason_and_reports_no_uncertainty() {
    let m = read(&db(), "");
    let edge = &m["definition"]["populations"][0]["density"]["max_radius"];
    assert_eq!(edge["source"]["from"], "choice");
    assert_eq!(edge["source"]["reason"], "Beyond the farthest arm.");
    assert_eq!(edge["uncertainty"]["form"], "none_reported");
    assert!(
        edge.get("kind").is_none(),
        "a choice is no kind of value: {edge}"
    );
}

#[test]
fn a_release_without_a_galaxy_model_answers_404_naming_the_release() {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(STRUCTURES);
    let response = db.get("/v1/galaxy-model", "");
    assert_eq!(response.status, 404);
    let problem = body(&response);
    assert_eq!(problem["type"], "urn:univ:problem:galaxy-model-not-found");
    assert_eq!(problem["release"], 2);
    assert!(
        problem["detail"].as_str().unwrap().contains("Release 2"),
        "{problem}"
    );
}
