//! The science checks file, `rules/science-checks.yaml`: each check selects something of a
//! built release by one of a few generic kinds and states what it should be, with its citation.

use serde::Deserialize;
use univ_model::position::Frame;

/// The checks file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckFile {
    pub checks: Vec<Check>,
}

/// One check.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub select: Select,
    #[serde(default)]
    pub expect: Expect,
    /// Where the expected value comes from: at least one citation.
    pub cite: Vec<Citation>,
    /// How the expected value was derived from what is cited, where it was.
    #[serde(default)]
    pub note: Option<String>,
}

/// What a check that does not hold comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The release is wrong: the command exits non-zero.
    Fail,
    /// Worth a look, not a failure.
    Concern,
    /// Reported, not judged.
    Report,
}

/// A primary citation: the paper or catalogue, where in it, the sentence or row quoted, and
/// where and when it was fetched.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub paper: String,
    #[serde(default)]
    pub locator: Option<String>,
    pub quote: String,
    pub fetched: String,
}

/// An object of the release: by a designation its source gives it, or by its common name.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ObjectRef {
    Designation { source: String, identifier: String },
    Name { name: String },
}

impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Designation { source, identifier } => write!(f, "{source}:{identifier}"),
            Self::Name { name } => write!(f, "name {name:?}"),
        }
    }
}

/// An object left out of a check that lists objects, with the reason.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    pub source: String,
    pub identifier: String,
    pub reason: String,
}

/// What the preferred measurement of an object should be: its entry's source and its method.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedBy {
    pub source: String,
    pub method: String,
}

/// A point of a volume grid, in the grid's frame and unit, and the label expected there.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridPoint {
    pub name: String,
    pub at: [f64; 3],
    pub label: i64,
}

/// The frame a cited velocity is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VelocityFrame {
    Heliocentric,
    Cmb,
}

/// What a check selects, and so what number it finds.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Select {
    /// An object's placed distance, Mpc; optionally which source and method placed it.
    ObjectDistance {
        object: ObjectRef,
        #[serde(default)]
        placed_by: Option<PlacedBy>,
    },
    /// An object's sky position: its separation from the expected direction, arcmin.
    ObjectDirection { object: ObjectRef },
    /// A researched structure's centre distance, Mpc, against the comoving distance of a cited
    /// velocity under the release cosmology, corrected to the CMB frame if heliocentric.
    StructureDistance {
        structure: String,
        velocity_km_s: f64,
        frame: VelocityFrame,
    },
    /// The labels of a grid field at points given in `frame` and `unit`, which the grid's
    /// header must name: the number of points whose label differs.
    GridLabels {
        grid: String,
        field: usize,
        frame: String,
        unit: String,
        points: Vec<GridPoint>,
    },
    /// An SQL query of the loaded release: the number of rows it gives (the first column an
    /// object id, so that exceptions can be left out), or with `scalar` the first value.
    Query {
        sql: String,
        #[serde(default)]
        scalar: bool,
        #[serde(default)]
        exceptions: Vec<Exception>,
    },
    /// The pairs of placed objects closer than `radius_arcmin` on the sky whose distances
    /// agree within their uncertainties: the number of pairs.
    ClosePairs {
        scale_domains: Vec<String>,
        kinds: Vec<String>,
        radius_arcmin: f64,
        #[serde(default)]
        exceptions: Vec<Exception>,
    },
    /// The largest tile file of the release, bytes.
    LargestTile {},
    /// The manifest's estimate of the release's D1 storage, bytes.
    D1Estimate {},
    /// The manifest's per-table fingerprints against the loaded rows: the number that differ.
    Fingerprints {},
    /// Rows of an SQL query, printed and not judged.
    Report { sql: String },
}

/// What the found value should be. Which fields a check takes depends on its kind.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// An expected value, with one of `sigma`, `absolute` or `relative`.
    pub value: Option<f64>,
    /// The cited one-sigma uncertainty of `value`: the found value agrees when it lies within
    /// `n_sigma` (2 unless given) of the cited and the object's own uncertainty in quadrature.
    pub sigma: Option<f64>,
    pub n_sigma: Option<f64>,
    /// The most the found value may differ by, in its unit.
    pub absolute: Option<f64>,
    /// The most the found value may differ by, as a share of the expected one.
    pub relative: Option<f64>,
    /// The Hubble constant, km/s/Mpc, `value` and `sigma` are cited at: both are carried to the
    /// release's Hubble constant as distances at a fixed redshift are, by H0 / H0(release).
    pub hubble_constant: Option<f64>,
    pub equals: Option<f64>,
    pub at_least: Option<f64>,
    pub at_most: Option<f64>,
    /// An expected direction, degrees, in `frame`, within `within_arcmin`.
    pub frame: Option<Frame>,
    pub lon: Option<f64>,
    pub lat: Option<f64>,
    pub within_arcmin: Option<f64>,
}

impl Expect {
    /// Whether the fields given are those of a bound on a number found.
    fn is_bound(&self) -> bool {
        self.value.is_none()
            && self.frame.is_none()
            && (self.equals.is_some() || self.at_least.is_some() || self.at_most.is_some())
    }

    fn is_tolerance(&self) -> bool {
        let given = [self.sigma, self.absolute, self.relative]
            .iter()
            .filter(|t| t.is_some())
            .count();
        given == 1 && self.equals.is_none() && self.at_least.is_none() && self.at_most.is_none()
    }
}

impl Check {
    /// Checks that the check states what its kind needs, before anything is loaded.
    ///
    /// # Errors
    ///
    /// Names the check and what it lacks.
    pub fn validate(&self) -> Result<(), String> {
        let e = &self.expect;
        let fine = match &self.select {
            Select::ObjectDistance { .. } => e.value.is_some() && e.is_tolerance(),
            Select::StructureDistance { .. } => e.value.is_none() && e.relative.is_some(),
            Select::ObjectDirection { .. } => {
                e.frame.is_some() && e.lon.is_some() && e.lat.is_some() && e.within_arcmin.is_some()
            }
            Select::Report { .. } => self.severity == Severity::Report,
            Select::Query {
                scalar, exceptions, ..
            } => e.is_bound() && (!*scalar || exceptions.is_empty()),
            _ => e.is_bound(),
        };
        let blank = |t: &str| t.trim().is_empty();
        if self.cite.is_empty()
            || self
                .cite
                .iter()
                .any(|c| blank(&c.paper) || blank(&c.quote) || blank(&c.fetched))
        {
            return Err(format!(
                "check {} needs a citation with its paper, quote and where it was fetched",
                self.id
            ));
        }
        let exceptions = match &self.select {
            Select::Query { exceptions, .. } | Select::ClosePairs { exceptions, .. } => {
                exceptions.as_slice()
            }
            _ => &[],
        };
        if exceptions.iter().any(|x| blank(&x.reason)) || blank(&self.title) {
            return Err(format!(
                "check {} needs a title and a reason for each exception",
                self.id
            ));
        }
        if self.severity == Severity::Report && !matches!(self.select, Select::Report { .. }) {
            return Err(format!(
                "check {}: only a report has severity report",
                self.id
            ));
        }
        if fine {
            Ok(())
        } else {
            Err(format!(
                "check {} does not state the expectation its kind takes",
                self.id
            ))
        }
    }
}

/// Reads and validates the checks file.
///
/// # Errors
///
/// Fails on a file that cannot be read or parsed, a check that does not validate, or two
/// checks with one id.
pub fn read(path: &std::path::Path) -> Result<CheckFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: CheckFile =
        serde_norway::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut ids = std::collections::HashSet::new();
    for check in &file.checks {
        check.validate()?;
        if !ids.insert(check.id.as_str()) {
            return Err(format!("two checks have the id {}", check.id));
        }
    }
    Ok(file)
}
