//! How each access method asks for a chunk and reads the answer: `VizieR` tab-separated tables,
//! `SkyServer` and TAP CSV.

use sha2::{Digest, Sha256};
use url::form_urlencoded::Serializer;

use crate::source::{Access, SourceDefinition};

/// One chunk of a query: its name in events and provenance, its file on disk, and what is
/// appended to each mirror to ask for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkRequest {
    pub item: String,
    pub file: String,
    pub suffix: String,
}

/// A table as a provider answered it, every cell kept as text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The chunk requests of a query source, in order; none for a file source.
#[must_use]
pub fn chunk_requests(definition: &SourceDefinition) -> Vec<ChunkRequest> {
    match &definition.access {
        Access::VizierTsv {
            table,
            constraints,
            slices,
            ..
        } => {
            let ranges = slices.as_ref().map_or_else(
                || vec![None],
                |s| {
                    let mut ranges = Vec::new();
                    let mut low = s.from;
                    while low <= s.to {
                        let high = low.saturating_add(s.step - 1).min(s.to);
                        ranges.push(Some((s.column.as_str(), low, high)));
                        low = high.saturating_add(1);
                        if high == s.to {
                            break;
                        }
                    }
                    ranges
                },
            );
            ranges
                .into_iter()
                .enumerate()
                .map(|(i, range)| {
                    let mut query = vizier_query(definition, table, constraints);
                    let item = match range {
                        Some((column, low, high)) => {
                            query.append_pair(column, &format!("{low}..{high}"));
                            format!("{column} {low} to {high}")
                        }
                        None => "the whole table".to_owned(),
                    };
                    request(item, i, "tsv", format!("?{}", query.finish()))
                })
                .collect()
        }
        Access::SkyserverSql { query, chunks, .. } => split(query, *chunks, skyserver_suffix),
        Access::Tap { query, chunks, .. } => split(query, *chunks, tap_suffix),
        Access::Files { .. } | Access::Repository { .. } => Vec::new(),
    }
}

/// A `VizieR` query for the definition's columns: the computed `_RAJ2000` and `_DEJ2000` are
/// asked for with `-out.add`, the others with `-out`.
fn vizier_query(
    definition: &SourceDefinition,
    table: &str,
    constraints: &[String],
) -> Serializer<'static, String> {
    let (computed, stored): (Vec<&str>, Vec<&str>) = definition
        .columns
        .iter()
        .map(|c| c.column.as_str())
        .partition(|c| c.starts_with('_'));
    let mut query = Serializer::new(String::new());
    query.append_pair("-source", table);
    query.append_pair("-out", &stored.join(","));
    if !computed.is_empty() {
        query.append_pair("-out.add", "_RAJ,_DEJ");
    }
    query.append_pair("-out.max", "unlimited");
    for constraint in constraints {
        let (key, value) = constraint.split_once('=').unwrap_or((constraint, ""));
        query.append_pair(key, value);
    }
    query
}

/// The chunks of a query whose text holds `{chunk}` and `{chunks}`.
fn split(query: &str, chunks: u32, suffix: fn(&str) -> String) -> Vec<ChunkRequest> {
    (0..chunks)
        .map(|i| {
            let text = query
                .replace("{chunks}", &chunks.to_string())
                .replace("{chunk}", &i.to_string());
            request(
                format!("chunk {} of {chunks}", i + 1),
                i,
                "csv",
                suffix(&text),
            )
        })
        .collect()
}

/// A chunk request whose file name carries a digest of what it asks, so that a chunk kept from
/// a query split another way is never taken for one of this split.
fn request(
    item: String,
    index: impl std::fmt::Display,
    extension: &str,
    suffix: String,
) -> ChunkRequest {
    let digest = Sha256::digest(suffix.as_bytes());
    let tag = crate::fetch::hex(&digest[..4]);
    ChunkRequest {
        item,
        file: format!("chunk-{index:0>4}-{tag}.{extension}"),
        suffix,
    }
}

fn skyserver_suffix(sql: &str) -> String {
    let mut q = Serializer::new(String::new());
    q.append_pair("cmd", sql);
    q.append_pair("format", "csv");
    format!("?{}", q.finish())
}

fn tap_suffix(adql: &str) -> String {
    let mut q = Serializer::new(String::new());
    q.append_pair("REQUEST", "doQuery");
    q.append_pair("LANG", "ADQL");
    q.append_pair("FORMAT", "csv");
    q.append_pair("QUERY", adql);
    format!("/sync?{}", q.finish())
}

/// The request for the number of rows a source's chunks must hold, where its definition gives
/// a count query.
#[must_use]
pub fn count_request(definition: &SourceDefinition) -> Option<ChunkRequest> {
    let (count, suffix): (&String, fn(&str) -> String) = match &definition.access {
        Access::SkyserverSql {
            count: Some(count), ..
        } => (count, skyserver_suffix),
        Access::Tap {
            count: Some(count), ..
        } => (count, tap_suffix),
        _ => return None,
    };
    Some(request(
        "row count".to_owned(),
        "count",
        "csv",
        suffix(count),
    ))
}

/// The single number a count query answered.
///
/// # Errors
///
/// Fails when the answer is not one row holding one whole number.
pub fn read_count(access: &Access, body: &[u8]) -> Result<u64, String> {
    let table = read_table(access, body)?;
    match table.rows.as_slice() {
        [row] if row.len() == 1 => row[0]
            .parse()
            .map_err(|e| format!("the count {:?} is not a whole number: {e}", row[0])),
        _ => Err(format!(
            "a count answers one row of one column, not {} rows",
            table.rows.len()
        )),
    }
}

/// Reads an answer of `access` into a table.
///
/// # Errors
///
/// Fails when the answer is not a table of that format, such as an error page.
pub fn read_table(access: &Access, body: &[u8]) -> Result<Table, String> {
    let text = std::str::from_utf8(body).map_err(|e| format!("the answer is not UTF-8: {e}"))?;
    match access {
        Access::VizierTsv { .. } => read_vizier_tsv(text),
        Access::SkyserverSql { .. } | Access::Tap { .. } => read_csv(text),
        Access::Files { .. } | Access::Repository { .. } => {
            Err("files are read by their own readers".to_owned())
        }
    }
}

/// Checks that an answer is a table holding every column `definition` takes, so an error page
/// sent with status 200 is retried rather than kept as a chunk.
///
/// # Errors
///
/// Fails, saying why, when the answer is not such a table.
pub fn check_answer(definition: &SourceDefinition, body: &[u8]) -> Result<(), String> {
    let table = read_table(&definition.access, body)?;
    let missing: Vec<&str> = definition
        .columns
        .iter()
        .map(|c| c.column.as_str())
        .filter(|name| !table.columns.iter().any(|c| c == name))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the answer lacks the columns {}",
            missing.join(", ")
        ))
    }
}

/// `VizieR`'s `asu-tsv`: `#` comments, then a header, a units line and a line of dashes, then
/// one tab-separated row per line.
fn read_vizier_tsv(text: &str) -> Result<Table, String> {
    let mut lines = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    let header = lines.next().ok_or("the answer holds no table")?;
    let columns: Vec<String> = header.split('\t').map(|c| c.trim().to_owned()).collect();
    lines.next().ok_or("the table has no units line")?;
    let dashes = lines.next().ok_or("the table has no line of dashes")?;
    if !dashes.trim_start().starts_with('-') {
        return Err(format!(
            "expected a line of dashes under the header, found {dashes:?}"
        ));
    }
    let rows = lines
        .map(|line| {
            let row: Vec<String> = line.split('\t').map(|c| c.trim().to_owned()).collect();
            if row.len() == columns.len() {
                Ok(row)
            } else {
                Err(format!(
                    "a row has {} cells for {} columns: {line:?}",
                    row.len(),
                    columns.len()
                ))
            }
        })
        .collect::<Result<_, _>>()?;
    Ok(Table { columns, rows })
}

/// CSV as `SkyServer` and TAP services give it: `#` lines skipped, a header, then rows, with
/// fields in double quotes where they hold a comma.
fn read_csv(text: &str) -> Result<Table, String> {
    let mut lines = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    let header = lines.next().ok_or("the answer holds no table")?;
    if header.trim_start().starts_with('<') {
        return Err(format!(
            "the answer is markup, not CSV: {}",
            header.chars().take(120).collect::<String>()
        ));
    }
    let columns = fields(header);
    let rows = lines
        .map(|line| {
            let row = fields(line);
            if row.len() == columns.len() {
                Ok(row)
            } else {
                Err(format!(
                    "a row has {} fields for {} columns: {line:?}",
                    row.len(),
                    columns.len()
                ))
            }
        })
        .collect::<Result<_, _>>()?;
    Ok(Table { columns, rows })
}

/// The fields of one CSV line.
fn fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut field).trim().to_owned()),
            _ => field.push(c),
        }
    }
    fields.push(field.trim().to_owned());
    fields
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn csv_fields_keep_quoted_commas_and_doubled_quotes() {
        assert_eq!(
            fields(r#"1,"a, b","say ""hi""",  2.5 "#),
            ["1", "a, b", "say \"hi\"", "2.5"]
        );
    }
}
