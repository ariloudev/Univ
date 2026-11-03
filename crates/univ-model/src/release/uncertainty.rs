//! An [`Uncertainty`] as four columns: its form, a term, and `minus`, `plus` and `level`.
//!
//! | Form | minus | plus | level |
//! |---|---|---|---|
//! | `none_reported`, `upper_limit`, `lower_limit` | | | |
//! | `symmetric` | sigma | sigma | |
//! | `asymmetric` | minus | plus | |
//! | `interval` | minus | plus | |
//! | `credible_region` | | | probability |
//! | `credible_interval` | minus | plus | probability |

use crate::value::Uncertainty;

/// An uncertainty's columns: its form's name and the numbers the form takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UncertaintyColumns {
    /// The form, as univ-model serializes it, such as `credible_interval`.
    pub form: &'static str,
    pub minus: Option<f64>,
    pub plus: Option<f64>,
    pub level: Option<f64>,
}

impl From<Uncertainty> for UncertaintyColumns {
    fn from(u: Uncertainty) -> Self {
        let columns = |form, minus, plus, level| Self {
            form,
            minus,
            plus,
            level,
        };
        match u {
            Uncertainty::NoneReported => columns("none_reported", None, None, None),
            Uncertainty::UpperLimit => columns("upper_limit", None, None, None),
            Uncertainty::LowerLimit => columns("lower_limit", None, None, None),
            Uncertainty::Symmetric { sigma } => {
                columns("symmetric", Some(sigma), Some(sigma), None)
            }
            Uncertainty::Asymmetric { minus, plus } => {
                columns("asymmetric", Some(minus), Some(plus), None)
            }
            Uncertainty::Interval { minus, plus } => {
                columns("interval", Some(minus), Some(plus), None)
            }
            Uncertainty::CredibleRegion { probability } => {
                columns("credible_region", None, None, Some(probability))
            }
            Uncertainty::CredibleInterval {
                minus,
                plus,
                probability,
            } => columns(
                "credible_interval",
                Some(minus),
                Some(plus),
                Some(probability),
            ),
        }
    }
}

impl UncertaintyColumns {
    /// The uncertainty the columns hold: none when the form is unknown or lacks a number it
    /// takes.
    #[must_use]
    pub fn uncertainty(&self) -> Option<Uncertainty> {
        Some(match self.form {
            "none_reported" => Uncertainty::NoneReported,
            "upper_limit" => Uncertainty::UpperLimit,
            "lower_limit" => Uncertainty::LowerLimit,
            "symmetric" => Uncertainty::Symmetric { sigma: self.minus? },
            "asymmetric" => Uncertainty::Asymmetric {
                minus: self.minus?,
                plus: self.plus?,
            },
            "interval" => Uncertainty::Interval {
                minus: self.minus?,
                plus: self.plus?,
            },
            "credible_region" => Uncertainty::CredibleRegion {
                probability: self.level?,
            },
            "credible_interval" => Uncertainty::CredibleInterval {
                minus: self.minus?,
                plus: self.plus?,
                probability: self.level?,
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_reads_back_from_its_columns() {
        let all = [
            Uncertainty::NoneReported,
            Uncertainty::UpperLimit,
            Uncertainty::LowerLimit,
            Uncertainty::Symmetric { sigma: 0.25 },
            Uncertainty::Asymmetric {
                minus: 1.5,
                plus: 2.5,
            },
            Uncertainty::Interval {
                minus: 0.06,
                plus: 0.06,
            },
            Uncertainty::CredibleRegion { probability: 0.95 },
            Uncertainty::CredibleInterval {
                minus: 18.25,
                plus: 17.75,
                probability: 0.68,
            },
        ];
        for u in all {
            let columns = UncertaintyColumns::from(u);
            assert_eq!(columns.uncertainty(), Some(u), "{u:?}");
            // The form is the name univ-model gives it.
            let json = serde_norway::to_string(&u).unwrap();
            assert!(json.contains(&format!("form: {}", columns.form)), "{json}");
        }
        let unknown = UncertaintyColumns {
            form: "gaussian",
            minus: None,
            plus: None,
            level: None,
        };
        assert_eq!(unknown.uncertainty(), None);
    }
}
