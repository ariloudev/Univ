//! Reading SQL text for the JSON functions it applies and what each applies to.
//!
//! The SQL is tokenized, not parsed: each statement's `FROM` and `JOIN` lists name its tables and
//! their aliases, and each column a JSON function's first argument names is looked up in the
//! release schema through them.

use std::collections::HashMap;

use super::tokens::{Kind, Token, tokens};

use univ_model::release::{ColumnKind, TABLES, Table};

/// SQLite's JSON functions that read the JSON given as their first argument.
const READERS: &[&str] = &[
    "json",
    "jsonb",
    "json_extract",
    "jsonb_extract",
    "json_each",
    "jsonb_each",
    "json_tree",
    "jsonb_tree",
    "json_type",
    "json_array_length",
    "json_valid",
    "json_patch",
    "jsonb_patch",
    "json_remove",
    "jsonb_remove",
    "json_replace",
    "jsonb_replace",
    "json_set",
    "jsonb_set",
    "json_insert",
    "jsonb_insert",
    "json_error_position",
    "json_pretty",
];

/// The table-valued JSON functions, whose `value` column is JSON already read.
const EACH: &[&str] = &["json_each", "jsonb_each", "json_tree", "jsonb_tree"];

/// A JSON function the gate refuses: where its text starts in the SQL, the call, and why.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub at: usize,
    pub call: String,
    pub why: String,
}

/// Every JSON function of `sql` applied to anything but a bound parameter, a literal, the
/// `value` of a `json_each` or a JSON column of a table bounded by the release's sources,
/// structures and chains.
pub(super) fn refusals(sql: &str) -> Vec<Found> {
    let tokens = tokens(sql);
    tokens
        .split(|t| t.is_punct(";"))
        .flat_map(|statement| Statement::new(statement).refusals(sql))
        .collect()
}

/// What a name of a statement's `FROM` or `JOIN` lists reads.
#[derive(Clone, Debug)]
enum Source {
    Table(&'static Table),
    /// A `json_each` or `json_tree`.
    Each,
    /// A table or function the release schema does not hold, such as a common table expression.
    Other(String),
}

struct Statement<'t, 'a> {
    tokens: &'t [Token<'a>],
    /// What each table name and alias, in lower case, reads: an alias given again in a subquery
    /// reads each.
    sources: HashMap<String, Vec<Source>>,
    /// Whether it reads a `json_each` or `json_tree`, whose `value` it may name bare.
    each: bool,
    /// Whether it reads any table at all: with no `FROM` or `JOIN` it reads no stored column.
    reads_tables: bool,
}

impl<'t, 'a> Statement<'t, 'a> {
    fn new(tokens: &'t [Token<'a>]) -> Self {
        let mut sources: HashMap<String, Vec<Source>> = HashMap::new();
        let mut bind = |name: &str, source: Source| {
            sources
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(source);
        };
        let mut reads_tables = false;
        for (i, t) in tokens.iter().enumerate() {
            if !(t.is_word("from") || t.is_word("join")) {
                continue;
            }
            reads_tables = true;
            let mut j = i + 1;
            while let Some(name) = tokens.get(j).filter(|t| t.is_name()) {
                let lower = name.text.to_ascii_lowercase();
                // A table-valued function: a json_each is named below, wherever it stands.
                let function = tokens.get(j + 1).is_some_and(|t| t.is_punct("("));
                let source = if function {
                    j = closing(tokens, j + 1) + 1;
                    (!EACH.contains(&lower.as_str())).then_some(Source::Other(lower))
                } else {
                    j += 1;
                    let source = TABLES
                        .iter()
                        .find(|t| t.name == lower)
                        .map_or_else(|| Source::Other(lower.clone()), Source::Table);
                    bind(&lower, source.clone());
                    Some(source)
                };
                if tokens.get(j).is_some_and(|t| t.is_word("as")) {
                    j += 1;
                }
                if let Some(alias) = tokens.get(j).filter(|t| t.is_name()) {
                    if let Some(source) = source {
                        bind(alias.text, source);
                    }
                    j += 1;
                }
                if !tokens.get(j).is_some_and(|t| t.is_punct(",")) {
                    break;
                }
                j += 1;
            }
        }
        // A `json_each`, which may also follow a join's `ON` after a comma: its alias is after
        // its `)`.
        let mut each = false;
        for (i, t) in tokens.iter().enumerate() {
            let function = EACH.contains(&t.text.to_ascii_lowercase().as_str())
                && t.kind == Kind::Word
                && tokens.get(i + 1).is_some_and(|t| t.is_punct("("));
            if !function {
                continue;
            }
            each = true;
            let mut j = closing(tokens, i + 1) + 1;
            if tokens.get(j).is_some_and(|t| t.is_word("as")) {
                j += 1;
            }
            if let Some(alias) = tokens.get(j).filter(|t| t.is_name()) {
                bind(alias.text, Source::Each);
            }
        }
        Self {
            tokens,
            sources,
            each,
            reads_tables,
        }
    }

    fn refusals(&self, sql: &str) -> Vec<Found> {
        let mut found = Vec::new();
        if !self.reads_tables {
            return found;
        }
        let tokens = self.tokens;
        for (i, t) in tokens.iter().enumerate() {
            let reader = t.kind == Kind::Word
                && READERS.contains(&t.text.to_ascii_lowercase().as_str())
                && tokens.get(i + 1).is_some_and(|t| t.is_punct("("));
            if reader {
                let end = closing_at(tokens, i + 2, true);
                if end <= i + 2 {
                    continue;
                }
                let last = tokens[end - 1];
                if let Err(why) = self.reads(&tokens[i + 2..end]) {
                    let rest = if tokens.get(end).is_some_and(|t| t.is_punct(",")) {
                        ", …)"
                    } else {
                        ")"
                    };
                    let call = format!("{}{rest}", &sql[t.at..last.end()]);
                    found.push(Found {
                        at: t.at,
                        call,
                        why,
                    });
                }
            }
            let arrow = (t.is_punct("->") || t.is_punct("->>"))
                && tokens
                    .get(i + 1)
                    .is_some_and(|t| matches!(t.kind, Kind::Literal | Kind::Param));
            if arrow {
                let (from, why) = self.left_of(i);
                if let Some(why) = why {
                    let call = sql[tokens[from].at..tokens[i + 1].end()].to_owned();
                    found.push(Found {
                        at: tokens[from].at,
                        call,
                        why,
                    });
                }
            }
        }
        found
    }

    /// The first token of what the arrow at `i` applies to, and why it is refused, if it is.
    fn left_of(&self, i: usize) -> (usize, Option<String>) {
        let tokens = self.tokens;
        let Some(left) = i.checked_sub(1).map(|l| tokens[l]) else {
            return (i, Some("it applies to nothing".to_owned()));
        };
        match left.kind {
            Kind::Param | Kind::Literal => (i - 1, None),
            Kind::Word if i >= 3 && tokens[i - 2].is_punct(".") => {
                (i - 3, self.reads(&tokens[i - 3..i]).err())
            }
            Kind::Word => (i - 1, self.reads(&tokens[i - 1..i]).err()),
            Kind::Punct => (
                i - 1,
                Some(format!("cannot tell what {} applies to", tokens[i].text)),
            ),
        }
    }

    /// Whether the expression `tokens`, a JSON function's argument, reads only parameters,
    /// literals, a `json_each`'s value or JSON columns of tables that may hold JSON.
    fn reads(&self, tokens: &[Token]) -> Result<(), String> {
        let mut k = 0;
        while k < tokens.len() {
            let t = tokens[k];
            k += 1;
            if !t.is_name() || tokens.get(k).is_some_and(|t| t.is_punct("(")) {
                continue;
            }
            if tokens.get(k).is_some_and(|t| t.is_punct("."))
                && let Some(column) = tokens.get(k + 1).filter(|t| t.kind == Kind::Word)
            {
                k += 2;
                self.qualified(t.text, column.text)?;
                continue;
            }
            self.bare(t.text)?;
        }
        Ok(())
    }

    /// Whether `alias.column` is JSON the gate allows, for each source the alias names.
    fn qualified(&self, alias: &str, column: &str) -> Result<(), String> {
        let column = column.to_ascii_lowercase();
        let sources = self
            .sources
            .get(&alias.to_ascii_lowercase())
            .ok_or_else(|| {
                format!(
                    "cannot tell which table {alias} is: name it in the statement's FROM or JOIN"
                )
            })?;
        sources.iter().try_for_each(|source| match source {
            Source::Each if column == "value" => Ok(()),
            Source::Each => Err(format!(
                "{alias}.{column} is not the value of the json_each it names"
            )),
            Source::Table(table) => holds_json(table, &column),
            Source::Other(name) => Err(format!(
                "{alias}.{column} is a column of {name}, which is no table of the release schema"
            )),
        })
    }

    fn bare(&self, column: &str) -> Result<(), String> {
        let column = column.to_ascii_lowercase();
        if column == "value" && self.each {
            return Ok(());
        }
        let holding: Vec<&Table> = self
            .sources
            .values()
            .flatten()
            .filter_map(|s| match s {
                Source::Table(t) if t.position(&column).is_some() => Some(*t),
                _ => None,
            })
            .collect();
        if holding.is_empty() {
            return Err(format!(
                "cannot tell which table holds {column}: none the statement names has it"
            ));
        }
        holding.iter().try_for_each(|t| holds_json(t, &column))
    }
}

/// Whether `table.column` is a JSON column of a table that may hold JSON.
fn holds_json(table: &Table, column: &str) -> Result<(), String> {
    let name = table.name;
    let scale = table.scale.words();
    match table.position(column).map(|p| table.kinds[p]) {
        None => Err(format!("{name} has no column {column}")),
        Some(_) if !table.scale.allows_json() => Err(format!(
            "{name} is a table {scale}: read {column} as a column, or the rows that replaced the \
             JSON, through an index"
        )),
        Some(ColumnKind::Json) => Ok(()),
        Some(_) => Err(format!("{name}.{column} is not a JSON column")),
    }
}

/// The index of the `)` closing the `(` at `open`, or the last token.
fn closing(tokens: &[Token], open: usize) -> usize {
    closing_at(tokens, open + 1, false).min(tokens.len().saturating_sub(1))
}

/// The index of the `)` that closes the parenthesis `from` is in, or with `comma` of the first
/// `,` at its depth, whichever comes first; the number of tokens when there is none.
fn closing_at(tokens: &[Token], from: usize, comma: bool) -> usize {
    let mut depth = 0_usize;
    for (k, t) in tokens.iter().enumerate().skip(from) {
        if t.is_punct("(") {
            depth += 1;
        } else if depth == 0 && (t.is_punct(")") || (comma && t.is_punct(","))) {
            return k;
        } else if t.is_punct(")") {
            depth -= 1;
        }
    }
    tokens.len()
}
