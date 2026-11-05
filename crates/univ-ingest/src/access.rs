//! How each access method asks for a chunk and reads the answer: `VizieR` tab-separated tables,
//! `SkyServer` and TAP CSV.

use sha2::{Digest, Sha256};
use url::form_urlencoded::Serializer;

use crate::fetch::Ask;
use crate::source::{Access, SourceDefinition, TapRun};
use crate::table::TableReader;

/// One chunk of a query: its name in events and provenance, its file on disk, and how each
/// mirror is asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkRequest {
    pub item: String,
    pub file: String,
    pub ask: Ask,
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
            let ranges: Vec<_> = slices.as_ref().map_or_else(
                || vec![None],
                |s| {
                    let step = usize::try_from(s.step).unwrap_or(usize::MAX);
                    (s.from..=s.to)
                        .step_by(step)
                        .map(|low| {
                            Some((
                                s.column.as_str(),
                                low,
                                low.saturating_add(s.step - 1).min(s.to),
                            ))
                        })
                        .collect()
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
                    request(item, i, "tsv", Ask::Get(format!("?{}", query.finish())))
                })
                .collect()
        }
        Access::SkyserverSql { query, chunks, .. } | Access::Tap { query, chunks, .. } => {
            ask_for(&definition.access).map_or_else(Vec::new, |a| split(query, *chunks, a))
        }
        Access::Files { .. } | Access::Repository { .. } => Vec::new(),
    }
}

/// A `VizieR` query for the definition's columns: the computed ones, whose names start with
/// `_`, are asked for with `-out.add`, the others with `-out`. `VizieR` answers `_RAJ` and `_DEJ`
/// as the columns `_RAJ2000` and `_DEJ2000`.
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
        let asked: Vec<&str> = computed
            .iter()
            .map(|c| match *c {
                "_RAJ2000" => "_RAJ",
                "_DEJ2000" => "_DEJ",
                other => other,
            })
            .collect();
        query.append_pair("-out.add", &asked.join(","));
    }
    query.append_pair("-out.max", "unlimited");
    for constraint in constraints {
        let (key, value) = constraint.split_once('=').unwrap_or((constraint, ""));
        query.append_pair(key, value);
    }
    query
}

/// The chunks of a query whose text holds `{chunk}` and `{chunks}`.
fn split(query: &str, chunks: u32, ask: fn(&str) -> Ask) -> Vec<ChunkRequest> {
    (0..chunks)
        .map(|i| {
            let text = query
                .replace("{chunks}", &chunks.to_string())
                .replace("{chunk}", &i.to_string());
            request(format!("chunk {} of {chunks}", i + 1), i, "csv", ask(&text))
        })
        .collect()
}

/// A chunk request whose file name carries a digest of what it asks, so that a chunk kept from
/// a query split another way is never taken for one of this split.
fn request(item: String, index: impl std::fmt::Display, extension: &str, ask: Ask) -> ChunkRequest {
    let digest = Sha256::digest(ask.suffix().as_bytes());
    let tag = crate::fetch::hex(&digest[..4]);
    ChunkRequest {
        item,
        file: format!("chunk-{index:0>4}-{tag}.{extension}"),
        ask,
    }
}

/// How a mirror is asked a query of `access`, for the methods that take one.
fn ask_for(access: &Access) -> Option<fn(&str) -> Ask> {
    match access {
        Access::SkyserverSql { .. } => Some(skyserver),
        Access::Tap { run, .. } => Some(match run {
            TapRun::Sync => tap,
            TapRun::Async => tap_job,
        }),
        Access::VizierTsv { .. } | Access::Files { .. } | Access::Repository { .. } => None,
    }
}

fn skyserver(sql: &str) -> Ask {
    let mut q = Serializer::new(String::new());
    q.append_pair("cmd", sql);
    q.append_pair("format", "csv");
    Ask::Get(format!("?{}", q.finish()))
}

fn tap(adql: &str) -> Ask {
    Ask::Get(format!("/sync?{}", tap_form(adql)))
}

fn tap_job(adql: &str) -> Ask {
    Ask::Job(format!("{}&PHASE=RUN", tap_form(adql)))
}

fn tap_form(adql: &str) -> String {
    let mut q = Serializer::new(String::new());
    q.append_pair("REQUEST", "doQuery");
    q.append_pair("LANG", "ADQL");
    q.append_pair("FORMAT", "csv");
    q.append_pair("QUERY", adql);
    q.finish()
}

/// The request for the number of rows a source's chunks must hold, where its definition gives
/// a count query. A count is one row, asked at once even of a source whose chunks run as jobs.
#[must_use]
pub fn count_request(definition: &SourceDefinition) -> Option<ChunkRequest> {
    let (Access::SkyserverSql {
        count: Some(count), ..
    }
    | Access::Tap {
        count: Some(count), ..
    }) = &definition.access
    else {
        return None;
    };
    let ask = match &definition.access {
        Access::Tap { .. } => tap,
        _ => ask_for(&definition.access)?,
    };
    Some(request("row count".to_owned(), "count", "csv", ask(count)))
}

/// The single number a count query answered.
///
/// # Errors
///
/// Fails when the answer is not one row holding one whole number.
pub fn read_count(access: &Access, body: &[u8]) -> Result<u64, String> {
    let mut table = TableReader::new(access, body)?;
    let count = match table.next_row() {
        Some(Ok([cell])) => cell
            .parse()
            .map_err(|e| format!("the count {cell:?} is not a whole number: {e}"))?,
        Some(Err(e)) => return Err(e),
        Some(Ok(_)) | None => {
            return Err("a count answers one row of one column".to_owned());
        }
    };
    match table.count_rows()? {
        0 => Ok(count),
        more => Err(format!(
            "a count answers one row of one column, not {} rows",
            more + 1
        )),
    }
}

/// Checks that an answer is a table holding every column `definition` takes, so an error page
/// sent with status 200 is never kept as a chunk, and gives its number of rows. An empty table,
/// with no header, is a valid answer of no rows.
///
/// # Errors
///
/// Fails, saying why, when the answer is not such a table.
pub fn check_answer(definition: &SourceDefinition, body: &[u8]) -> Result<u64, String> {
    let table = TableReader::new(&definition.access, body)?;
    if table.columns().is_empty() {
        return Ok(0);
    }
    let missing: Vec<&str> = definition
        .columns
        .iter()
        .map(|c| c.column.as_str())
        .filter(|name| table.index(name).is_none())
        .collect();
    if missing.is_empty() {
        table.count_rows()
    } else {
        Err(format!(
            "the answer lacks the columns {}",
            missing.join(", ")
        ))
    }
}

/// The chunks, count query and files a fetch of `definition` asks for.
#[must_use]
pub fn item_count(definition: &SourceDefinition) -> u64 {
    let items = match &definition.access {
        Access::Files { files } => files.len(),
        Access::Repository { .. } => 0,
        _ => chunk_requests(definition).len() + usize::from(count_request(definition).is_some()),
    };
    items as u64
}
