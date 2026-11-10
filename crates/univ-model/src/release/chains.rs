//! A derived position's transformation chain held once per release as a template, its
//! per-object numbers in the object's columns.
//!
//! A template is the JSON array of the chain's steps, as univ-model serializes them, with:
//!
//! - the value of each per-object field ([`PER_OBJECT`]: `largest_difference`, `cmb_redshift`,
//!   `heliocentric_redshift`, `barycentric_redshift`, `redshift`, `factor`, `corrected_mas` and `parallax_mas`) replaced
//!   by `"$1"` to `"$4"` in the order they come in the JSON (steps in order, a step's fields
//!   sorted by name), filled from `chain_p1` to `chain_p4` of the
//!   object; a chain with more than [`PARAMS`] of them keeps the rest in its template;
//! - each field holding the position's native value ([`NATIVE_FIELDS`]: `native` of a
//!   published distance or parallax, `modulus` of a distance modulus) replaced by
//!   `"$native"`, filled with the native value of the object's preferred measurement.
//!
//! Objects whose chains differ only in those numbers share one template.

use serde_json::{Map, Number, Value};

use super::rows::PARAMS;

/// The fields of a step whose number differs from object to object.
pub const PER_OBJECT: [&str; 8] = [
    "largest_difference",
    "cmb_redshift",
    "heliocentric_redshift",
    "barycentric_redshift",
    "redshift",
    "factor",
    "corrected_mas",
    "parallax_mas",
];
/// The fields of a step that hold the position's native value.
pub const NATIVE_FIELDS: [&str; 2] = ["native", "modulus"];
/// What stands for the native value in a template.
pub const NATIVE: &str = "$native";

/// A chain as its template and the per-object numbers the template takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Templated {
    pub steps: Value,
    pub params: Vec<f64>,
}

/// The placeholder of the `k`-th parameter, from 1.
fn placeholder(k: usize) -> String {
    format!("${k}")
}

/// The template of `steps`, a JSON array of steps, whose native value is `native`.
#[must_use]
pub fn template(steps: &Value, native: &Value) -> Templated {
    let mut params = Vec::new();
    let mut each = |step: &Map<String, Value>| -> Map<String, Value> {
        step.iter()
            .map(|(key, value)| {
                let held = if NATIVE_FIELDS.contains(&key.as_str()) && value == native {
                    Value::String(NATIVE.to_owned())
                } else {
                    match value.as_f64() {
                        Some(x) if PER_OBJECT.contains(&key.as_str()) && params.len() < PARAMS => {
                            params.push(x);
                            Value::String(placeholder(params.len()))
                        }
                        _ => value.clone(),
                    }
                };
                (key.clone(), held)
            })
            .collect()
    };
    let steps = match steps {
        Value::Array(steps) => Value::Array(
            steps
                .iter()
                .map(|s| {
                    s.as_object()
                        .map_or_else(|| s.clone(), |m| Value::Object(each(m)))
                })
                .collect(),
        ),
        other => other.clone(),
    };
    Templated { steps, params }
}

/// The chain `template` gives with the object's parameters `params` (`chain_p1` to
/// `chain_p4`) and native value `native`; none when a parameter it takes is missing or not a
/// finite number.
#[must_use]
pub fn expand(template: &Value, params: &[Option<f64>], native: &Value) -> Option<Value> {
    let fill = |key: &str, value: &Value| -> Option<Value> {
        let Value::String(s) = value else {
            return Some(value.clone());
        };
        if NATIVE_FIELDS.contains(&key) && s == NATIVE {
            return Some(native.clone());
        }
        if PER_OBJECT.contains(&key)
            && let Some(k) = s.strip_prefix('$').and_then(|k| k.parse::<usize>().ok())
        {
            let x = (*params.get(k.checked_sub(1)?)?)?;
            return Number::from_f64(x).map(Value::Number);
        }
        Some(value.clone())
    };
    let Value::Array(steps) = template else {
        return Some(template.clone());
    };
    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        out.push(match step {
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, v)| Some((k.clone(), fill(k, v)?)))
                    .collect::<Option<Map<String, Value>>>()?,
            ),
            other => other.clone(),
        });
    }
    Some(Value::Array(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_chain_reads_back_from_its_template_parameters_and_native_value() {
        let native = json!({"value": 37.61, "unit": "magnitude",
            "uncertainty": {"form": "symmetric", "sigma": 0.53}});
        let chain = json!([
            {"step": "published", "native": native, "measure": "luminosity",
             "citation": "the source's own value"},
            {"step": "from_modulus", "modulus": native, "citation": "μ = 5 log10(d / 10 pc)"},
            {"step": "luminosity_to_comoving", "cmb_redshift": 0.0792,
             "redshift_kind": "spectroscopic", "citation": "Hogg 1999"},
            {"step": "rotated", "to": "galactic", "citation": "ESA 1997"},
        ]);
        let t = template(&chain, &native);
        assert_eq!(t.params, [0.0792]);
        assert_eq!(t.steps[0]["native"], NATIVE);
        assert_eq!(t.steps[1]["modulus"], NATIVE);
        assert_eq!(t.steps[2]["cmb_redshift"], "$1");
        assert_eq!(
            expand(&t.steps, &[Some(0.0792)], &native),
            Some(chain.clone())
        );
        // Another object of that chain shares the template.
        let mut other = chain.clone();
        other[2]["cmb_redshift"] = json!(0.0123);
        let mut other_native = native.clone();
        other_native["value"] = json!(35.0);
        other[0]["native"] = other_native.clone();
        other[1]["modulus"] = other_native.clone();
        let u = template(&other, &other_native);
        assert_eq!(u.steps, t.steps);
        assert_eq!(
            expand(&u.steps, &[Some(0.0123)], &other_native),
            Some(other)
        );
        assert_eq!(
            expand(&t.steps, &[None], &native),
            None,
            "a missing parameter"
        );
    }

    #[test]
    fn objects_of_barycentric_redshifts_share_one_template() {
        let chain = |cmb: f64, bary: f64| {
            json!([{"step": "comoving_from_barycentric_redshift", "cmb_redshift": cmb,
                    "barycentric_redshift": bary, "dipole": "planck2018"}])
        };
        let one = template(&chain(0.401, 0.4), &Value::Null);
        let two = template(&chain(0.802, 0.8), &Value::Null);
        assert_eq!(one.steps, two.steps);
        assert_eq!(
            one.params,
            [0.4, 0.401],
            "in the order of the fields' names"
        );
    }

    #[test]
    fn numbers_beyond_four_parameters_stay_in_the_template() {
        let chain = json!([{"step": "s", "redshift": 1.0, "factor": 2.0},
            {"step": "t", "cmb_redshift": 3.0, "heliocentric_redshift": 4.0},
            {"step": "u", "largest_difference": 5.0}]);
        let t = template(&chain, &Value::Null);
        // In the order of the template's JSON, whose fields are sorted by name.
        assert_eq!(t.params, [2.0, 1.0, 3.0, 4.0]);
        assert_eq!(t.steps[2]["largest_difference"], 5.0);
        let p: Vec<Option<f64>> = t.params.iter().copied().map(Some).collect();
        assert_eq!(expand(&t.steps, &p, &Value::Null), Some(chain));
    }
}
