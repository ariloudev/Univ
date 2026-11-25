//! What a cross-match works under, read once: the release configuration, the matching, ranking
//! and tile rules with each file's text, and the researched structures. The cross-match carries
//! it in its result, so the tiles and the release are built under the very rules, peculiar
//! velocity and structures the objects were matched under, and the release records the text
//! that was read, not the files as they stand later.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use univ_model::galaxy_model::Model;

use crate::designations::DesignationRules;
use crate::galaxy_model::{ModelFile, RULES_NAME};
use crate::matching::MatchingRules;
use crate::ranking::RankingRules;
use crate::release::{CONFIG_FILE, ReleaseConfig};
use crate::source::ScaleDomain;
use crate::structures::{Structure, read_all_named};
use crate::tiles::TileRules;
use crate::yaml::{FileError, parse_checked};

/// The rules files a release records, by name: each is `rules/{name}.yaml`.
pub const RULES: [&str; 6] = [
    "matching",
    "ranking",
    "tiles",
    "designations",
    "release",
    RULES_NAME,
];

/// What a cross-match works under.
#[derive(Clone, Debug, PartialEq)]
pub struct Basis {
    pub config: ReleaseConfig,
    pub matching: MatchingRules,
    pub ranking: RankingRules,
    pub tiles: TileRules,
    pub designations: DesignationRules,
    /// The Galaxy model the release samples (build-the-galaxy-model).
    pub galaxy_model: Model,
    /// Each rules file as read, by its name in [`RULES`]; the release configuration as the
    /// default gives it where the repository has no file.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The researched structures, each by its id.
    pub structures: Vec<(String, Structure)>,
    /// The ids of the researched structures a build left out for their scale domain
    /// ([`Basis::keep_domains`]), which a membership may name without being refused.
    pub left_out: Vec<String>,
}

/// The text of `file`, read once, parsed with `check`.
fn parsed<T: serde::de::DeserializeOwned>(
    file: &Path,
    bytes: &[u8],
    check: impl FnOnce(&T) -> Result<(), (String, String)>,
) -> Result<T, FileError> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| FileError::new(file, "(file)", e.to_string()))?;
    parse_checked(file, text, check)
}

impl Basis {
    /// Reads the rules in `rules`, each file once, and the researched structures in each of
    /// `structures`.
    ///
    /// # Errors
    ///
    /// Fails, naming the file and field, when a rules file or structure cannot be read or used.
    pub fn read(rules: &Path, structures: &[PathBuf]) -> Result<Self, String> {
        let mut files = BTreeMap::new();
        for name in RULES {
            let file = rules.join(format!("{name}.yaml"));
            match std::fs::read(&file) {
                Ok(bytes) => {
                    files.insert(name.to_owned(), bytes);
                }
                Err(_) if name == "release" && !file.exists() => {}
                Err(e) => return Err(format!("{}: {e}", file.display())),
            }
        }
        let text = |name: &str| files.get(name).map_or(&[][..], Vec::as_slice);
        let at = |name: &str| rules.join(format!("{name}.yaml"));
        let matching = parsed(&at("matching"), text("matching"), MatchingRules::check);
        let ranking = parsed(&at("ranking"), text("ranking"), RankingRules::check);
        let tiles = parsed(&at("tiles"), text("tiles"), TileRules::check);
        let designations = parsed(
            &at("designations"),
            text("designations"),
            DesignationRules::check,
        );
        let config = match files.get("release") {
            Some(bytes) => parsed(&rules.join(CONFIG_FILE), bytes, ReleaseConfig::check),
            None => Ok(ReleaseConfig::default()),
        };
        let (matching, ranking, tiles, config) = (
            matching.map_err(|e| e.to_string())?,
            ranking.map_err(|e| e.to_string())?,
            tiles.map_err(|e| e.to_string())?,
            config.map_err(|e| e.to_string())?,
        );
        let designations = designations.map_err(|e| e.to_string())?;
        let galaxy_model = parsed(&at(RULES_NAME), text(RULES_NAME), ModelFile::check)
            .map_err(|e| e.to_string())?
            .model()?;
        if !files.contains_key("release") {
            let yaml = serde_norway::to_string(&config).map_err(|e| e.to_string())?;
            files.insert("release".to_owned(), yaml.into_bytes());
        }
        let mut named = Vec::new();
        for dir in structures {
            named.extend(read_all_named(dir).map_err(|e| e.to_string())?);
        }
        Ok(Self {
            config,
            matching,
            ranking,
            tiles,
            designations,
            galaxy_model,
            files,
            structures: named,
            left_out: Vec::new(),
        })
    }

    /// Keeps only the researched structures of `domains`, leaving out the others and naming
    /// them in [`Basis::left_out`]; keeps every one where `domains` is empty.
    pub fn keep_domains(&mut self, domains: &[ScaleDomain]) {
        if domains.is_empty() {
            return;
        }
        let (kept, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.structures)
            .into_iter()
            .partition(|(_, s)| domains.contains(&s.scale_domain));
        self.structures = kept;
        self.left_out = left.into_iter().map(|(id, _)| id).collect();
    }

    /// The text of the rules file `name`, as read.
    #[must_use]
    pub fn file(&self, name: &str) -> &[u8] {
        self.files.get(name).map_or(&[], Vec::as_slice)
    }

    /// The researched structures without their ids.
    #[must_use]
    pub fn structures(&self) -> Vec<Structure> {
        self.structures.iter().map(|(_, s)| s.clone()).collect()
    }
}
