//! The codes a store's rows are written in, read from its catalogue: each term by name and each
//! source by id, as every release it holds numbers them. A term the migration needs and no
//! release named is numbered after every term ever numbered, as a build numbers a new one.

use std::collections::HashMap;

use rusqlite::Connection;
use univ_model::ids::SourceId;

use super::super::extras::Codes;

/// The catalogue's terms and sources, and the terms numbered since they were read.
pub struct StoreCodes {
    release: u32,
    terms: HashMap<String, u32>,
    next: u32,
    added: Vec<(u32, String)>,
    sources: HashMap<String, u32>,
    unknown: Option<String>,
}

fn pairs<K: rusqlite::types::FromSql, V: rusqlite::types::FromSql>(
    db: &Connection,
    sql: &str,
) -> Result<Vec<(K, V)>, String> {
    let mut q = db.prepare(sql).map_err(|e| format!("{sql}: {e}"))?;
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("{sql}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{sql}: {e}"))
}

impl StoreCodes {
    /// The codes of the catalogue `catalogue`.
    ///
    /// # Errors
    ///
    /// Fails when its terms or sources cannot be read, or when one term or source has two
    /// numbers.
    pub fn read(catalogue: &Connection) -> Result<Self, String> {
        let mut terms = HashMap::new();
        let mut next = 0;
        for (id, term) in pairs::<u32, String>(catalogue, "SELECT id, term FROM terms")? {
            if let Some(other) = terms.insert(term.clone(), id).filter(|o| *o != id) {
                return Err(format!("term {term:?} is numbered both {other} and {id}"));
            }
            next = next.max(id + 1);
        }
        let mut sources = HashMap::new();
        for (id, n) in pairs::<String, u32>(catalogue, "SELECT id, n FROM sources")? {
            if let Some(other) = sources.insert(id.clone(), n).filter(|o| *o != n) {
                return Err(format!("source {id} is numbered both {other} and {n}"));
            }
        }
        Ok(Self {
            release: 0,
            terms,
            next,
            added: Vec::new(),
            sources,
            unknown: None,
        })
    }

    /// Writes the rows that follow in release `release`.
    pub fn at(&mut self, release: u32) {
        self.release = release;
    }

    /// The terms numbered since the last call, by number.
    pub fn take_added(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.added)
    }

    /// Fails when a row named a source the catalogue does not number.
    ///
    /// # Errors
    ///
    /// Names the first such source.
    pub fn check(&self) -> Result<(), String> {
        self.unknown.as_ref().map_or(Ok(()), |source| {
            Err(format!(
                "a row names source {source}, which no release of the store numbers"
            ))
        })
    }
}

impl Codes for StoreCodes {
    fn release(&self) -> u32 {
        self.release
    }

    fn term(&mut self, name: &str) -> u32 {
        if let Some(id) = self.terms.get(name) {
            return *id;
        }
        let id = self.next;
        self.next += 1;
        self.terms.insert(name.to_owned(), id);
        self.added.push((id, name.to_owned()));
        id
    }

    fn source(&mut self, source: &SourceId) -> u32 {
        if let Some(n) = self.sources.get(source.as_str()) {
            return *n;
        }
        self.unknown.get_or_insert_with(|| source.to_string());
        u32::MAX
    }
}
