//! The JSON gate, `cargo xtask json`
//! (keep-json-out-of-the-tables-that-grow-with-a-releases-objects):
//!
//! - **Schema:** no table of the release schema whose rows grow with the objects or the
//!   catalogue entries holds a JSON column.
//! - **SQL:** no JSON function of the project's SQL (Rust's string literals, the rules' YAML
//!   and `.sql` files) applies to anything but a bound parameter, a literal, the `value` of a
//!   `json_each`, or a JSON column of a table bounded by the release's sources, structures and
//!   chains; and no `->` or `->>` either.

mod rust;
mod sql;
#[cfg(test)]
mod tests;
mod tokens;

use std::fs;
use std::io;
use std::path::Path;

use univ_model::release::{ColumnKind, SCHEMA, TABLES, Table};

use crate::lines::listed;

/// Where the project's SQL is: Rust's string literals, the rules' YAML strings, SQL files.
const PATHSPECS: &[&str] = &["crates/*.rs", "xtask/*.rs", "rules/*.yaml", "*.sql"];

/// The schema whose tables the schema gate reads, shown as where they are declared.
const SCHEMA_FILE: &str = "crates/univ-model/src/release/schema.sql";

/// Something the gate refuses: the file and line, what is refused, and why.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub file: String,
    pub line: usize,
    pub what: String,
    pub why: String,
}

/// Runs both gates on the tree at `root`, naming each refusal; whether there was none.
pub(crate) fn gate(root: &Path) -> bool {
    let mut refused = schema(&TABLES, SCHEMA);
    match scan(root) {
        Ok(found) => refused.extend(found),
        Err(error) => {
            eprintln!("could not read the project's SQL: {error}");
            return false;
        }
    }
    for r in &refused {
        eprintln!("{}:{}: {}: {}", r.file, r.line, r.what, r.why);
    }
    refused.is_empty()
}

/// Each JSON column of `tables` in a table whose rows grow with the objects or the entries, at
/// the line of `schema` that creates its table.
fn schema(tables: &[Table], schema: &str) -> Vec<Refusal> {
    let mut refused = Vec::new();
    for table in tables.iter().filter(|t| !t.scale.allows_json()) {
        let created = format!(" {} (", table.name);
        let line = schema
            .lines()
            .position(|l| l.starts_with("CREATE TABLE") && l.contains(&created))
            .map_or(0, |n| n + 1);
        for (column, _) in table
            .columns
            .iter()
            .zip(table.kinds)
            .filter(|(_, kind)| **kind == ColumnKind::Json)
        {
            refused.push(Refusal {
                file: SCHEMA_FILE.to_owned(),
                line,
                what: format!("{}.{column}", table.name),
                why: format!(
                    "a JSON column in a table {}: give its values rows of their own",
                    table.scale.words()
                ),
            });
        }
    }
    refused
}

/// Every JSON function the project's SQL applies to what the gate refuses.
fn scan(root: &Path) -> io::Result<Vec<Refusal>> {
    let mut refused = Vec::new();
    for path in listed(root, PATHSPECS)? {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            // A tracked file deleted from the working tree is still listed.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io::Error::new(error.kind(), path.display().to_string())),
        };
        let file = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let found = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => in_rust(&text),
            Some("yaml") => in_yaml(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{file}: {e}")))?,
            _ => in_sql(&text),
        };
        refused.extend(found.into_iter().map(|(line, f)| Refusal {
            file: file.clone(),
            line,
            what: f.call,
            why: f.why,
        }));
    }
    Ok(refused)
}

/// The line of byte `at` in `text`, from 1.
fn line_of(text: &str, at: usize) -> usize {
    text.as_bytes()[..at.min(text.len())]
        .split(|b| *b == b'\n')
        .count()
}

fn in_sql(text: &str) -> Vec<(usize, sql::Found)> {
    sql::refusals(text)
        .into_iter()
        .map(|f| (line_of(text, f.at), f))
        .collect()
}

fn in_rust(code: &str) -> Vec<(usize, sql::Found)> {
    rust::literals(code)
        .into_iter()
        .flat_map(|(start, text)| {
            sql::refusals(&text)
                .into_iter()
                .map(move |f| (line_of(code, start + f.at), f))
        })
        .collect()
}

/// The refusals in each string of the YAML `text`, each at the line of the file that holds its
/// call. A string folded from several lines is no longer laid out as in the file, so the call is
/// found by its words: the nth time they come in the strings is the nth time in the file.
fn in_yaml(text: &str) -> Result<Vec<(usize, sql::Found)>, serde_norway::Error> {
    let value: serde_norway::Value = serde_norway::from_str(text)?;
    let mut strings = Vec::new();
    strings_of(&value, &mut strings);
    let mut found = Vec::new();
    for (i, string) in strings.iter().enumerate() {
        for f in sql::refusals(string) {
            let call = string[f.at..].split(['\n', ',']).next().unwrap_or_default();
            let before = strings[..i]
                .iter()
                .map(|s| s.matches(call).count())
                .sum::<usize>()
                + string[..f.at].matches(call).count();
            let at = text.match_indices(call).nth(before).map_or(0, |(at, _)| at);
            found.push((line_of(text, at), f));
        }
    }
    Ok(found)
}

/// Every string of `value`, in the order of the document.
fn strings_of<'v>(value: &'v serde_norway::Value, out: &mut Vec<&'v str>) {
    match value {
        serde_norway::Value::String(s) => out.push(s),
        serde_norway::Value::Sequence(items) => items.iter().for_each(|v| strings_of(v, out)),
        serde_norway::Value::Mapping(map) => map.values().for_each(|v| strings_of(v, out)),
        serde_norway::Value::Tagged(tagged) => strings_of(&tagged.value, out),
        _ => {}
    }
}
