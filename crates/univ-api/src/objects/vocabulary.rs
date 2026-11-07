//! A release's vocabulary (its terms, chain templates, sources and cosmology), read once and
//! kept by the backend: a release never changes once published.

use std::sync::Arc;

use serde_json::{Value, json};
use univ_model::cosmology::Cosmology;
use univ_model::release::read::{Reading, Vocabulary};
use univ_model::release::rows::CatalogueUncertainty;
use univ_model::valid_in;

use crate::backend::{Backend, column, first, json_column};
use crate::problem::{Problem, ProblemType};

/// The vocabulary of `release`, from the backend's keeping or read in one query.
///
/// # Errors
///
/// An internal-error problem when the release database cannot be read.
pub async fn vocabulary<B: Backend>(backend: &B, release: u32) -> Result<Arc<Vocabulary>, Problem> {
    if let Some(vocabulary) = backend.kept().vocabularies.get(&release) {
        return Ok(vocabulary);
    }
    let rows = backend
        .releases(
            concat!(
                "SELECT (SELECT cosmology FROM releases WHERE number = ?1) AS cosmology, \
                 (SELECT ranking_rule_name FROM releases WHERE number = ?1) AS ranking_rule, \
                 (SELECT json_group_array(json_array(id, term)) FROM terms WHERE ",
                valid_in!("", "?1"),
                ") AS terms, \
                 (SELECT json_group_array(json_array(id, json(steps))) FROM chains WHERE ",
                valid_in!("", "?1"),
                ") AS chains, \
                 (SELECT json_group_array(json_array(n, id)) FROM sources WHERE ",
                valid_in!("", "?1"),
                ") AS sources, \
                 (SELECT json_group_array(json_array(s.id, json(t.catalogue_uncertainties))) \
                   FROM source_terms t JOIN sources s ON s.n = t.source AND ",
                valid_in!("s.", "?1"),
                " WHERE t.catalogue_uncertainties IS NOT NULL AND ",
                valid_in!("t.", "?1"),
                ") AS uncertainties, \
                 (SELECT json_group_array(json_array(s.id, r.designation_form, r.rank)) \
                   FROM source_reading r JOIN sources s ON s.n = r.source AND ",
                valid_in!("s.", "?1"),
                " WHERE ",
                valid_in!("r.", "?1"),
                ") AS reading"
            ),
            &[json!(release)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| unreadable("no row"))?;
    let cosmology: Cosmology = json_column(&row, "cosmology")?;
    let ranking_rule: String = column(&row, "ranking_rule")?;
    let terms: Vec<(i64, String)> = json_column(&row, "terms")?;
    let chains: Vec<(i64, Value)> = json_column(&row, "chains")?;
    let sources: Vec<(i64, String)> = json_column(&row, "sources")?;
    let uncertainties: Vec<(String, Vec<CatalogueUncertainty>)> =
        json_column(&row, "uncertainties")?;
    let reading: Vec<(String, Option<String>, Option<u32>)> = json_column(&row, "reading")?;
    let vocabulary = Vocabulary::new(release, cosmology, ranking_rule, terms, chains, sources)
        .with_catalogue_uncertainties(
            uncertainties
                .into_iter()
                .flat_map(|(source, each)| each.into_iter().map(move |u| (source.clone(), u))),
        )
        .with_reading(Reading::new(reading));
    Ok(backend.kept().vocabularies.keep(release, vocabulary))
}

fn unreadable(why: &str) -> Problem {
    Problem::new(
        ProblemType::InternalError,
        format!("The release's vocabulary could not be read: {why}."),
    )
}
