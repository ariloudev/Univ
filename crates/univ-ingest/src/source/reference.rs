//! A source's primary reference: the paper its `citation` names first, by its ADS bibcode and
//! DOI, so that a release can name it in a form a reader can resolve. A source with no single
//! primary reference, such as the researched structures, whose papers are cited per structure,
//! says why instead.

use serde::{Deserialize, Serialize};

/// The primary reference as a definition writes it: `{ bibcode, doi }`, at least one of them, or
/// `{ none: <why> }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    /// The ADS bibcode, such as `2012ApJS..199...26H`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibcode: Option<String>,
    /// The DOI, such as `10.1088/0067-0049/199/2/26`, without a `doi:` prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// Why the source has no single primary reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub none: Option<String>,
}

/// Whether `bibcode` has the 19-character form of an ADS bibcode, `YYYYJJJJJVVVVMPPPPA`: a
/// four-digit year, then the journal, volume, section and page, padded with dots, and the first
/// author's initial (or a dot), with no space.
#[must_use]
pub fn is_bibcode(bibcode: &str) -> bool {
    let b = bibcode.as_bytes();
    b.len() == 19
        && b[..4].iter().all(u8::is_ascii_digit)
        && b.iter().all(u8::is_ascii_graphic)
        && (b[18].is_ascii_alphabetic() || b[18] == b'.')
}

/// Whether `doi` has the form of a DOI: the directory indicator `10.`, a registrant code, a
/// slash and a suffix, with no space and no `doi:` prefix.
#[must_use]
pub fn is_doi(doi: &str) -> bool {
    doi.strip_prefix("10.")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(registrant, suffix)| {
            !registrant.is_empty() && !suffix.is_empty() && !doi.chars().any(char::is_whitespace)
        })
}

impl Reference {
    /// The checks serde cannot make, as the field and the problem.
    pub(super) fn check(&self) -> Result<(), (String, String)> {
        let problem = |field: &str, what: String| Err((format!("reference.{field}"), what));
        match (&self.bibcode, &self.doi, &self.none) {
            (None, None, None) => {
                return Err((
                    "reference".to_owned(),
                    "gives no bibcode or DOI, nor why there is none".to_owned(),
                ));
            }
            (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
                return problem("none", "is given beside a bibcode or DOI".to_owned());
            }
            (None, None, Some(why)) if why.trim().is_empty() => {
                return problem("none", "is empty".to_owned());
            }
            _ => {}
        }
        if let Some(b) = &self.bibcode
            && !is_bibcode(b)
        {
            return problem(
                "bibcode",
                format!("{b} is not a 19-character ADS bibcode (YYYYJJJJJVVVVMPPPPA)"),
            );
        }
        if let Some(d) = &self.doi
            && !is_doi(d)
        {
            return problem(
                "doi",
                format!("{d} is not a DOI: 10.<registrant>/<suffix>, with no doi: prefix"),
            );
        }
        Ok(())
    }
}
