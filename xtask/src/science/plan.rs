//! The scan guard (keep-json-out-of-the-tables-that-grow-with-a-releases-objects): a query of a
//! check that reads a table per object or per catalogue entry whole fails, naming it.
//!
//! It reads the query's bytecode rather than its plan's text, which names tables by their
//! aliases: each cursor opened on the scratch database's B-trees names a table or index by its
//! root page, and a cursor rewound to its first row (`Rewind`, `Last`) or counted (`Count`) reads
//! it whole, where a search seeks into it. An automatic index is built by rewinding its table.

use std::collections::HashMap;

use rusqlite::{Connection, Statement};
use univ_model::release::TABLES;

/// Opcodes that open a cursor on a B-tree of the database, its root page in P2.
const OPEN: &[&str] = &["OpenRead", "ReopenIdx"];

/// Opcodes that read the B-tree under a cursor from one end, or count it.
const WHOLE: &[&str] = &["Rewind", "Last", "Count"];

/// `sql` prepared on `db`, with `:release` bound to `release` where it names it.
///
/// # Errors
///
/// Fails when `sql` cannot be prepared.
pub fn bound<'c>(db: &'c Connection, sql: &str, release: u32) -> Result<Statement<'c>, String> {
    let mut q = db.prepare(sql).map_err(|e| e.to_string())?;
    if let Some(i) = q.parameter_index(":release").map_err(|e| e.to_string())? {
        q.raw_bind_parameter(i, release)
            .map_err(|e| e.to_string())?;
    }
    Ok(q)
}

/// Each table per object or per catalogue entry that `sql` reads whole, with the B-tree it reads
/// it through: the table itself or one of its indexes.
///
/// # Errors
///
/// Fails when `sql` cannot be prepared.
pub fn whole_reads(db: &Connection, sql: &str, release: u32) -> Result<Vec<String>, String> {
    // Each B-tree of the main database by its root page: its name and its table's.
    let mut trees = db
        .prepare("SELECT rootpage, name, tbl_name FROM main.sqlite_schema WHERE rootpage > 0")
        .map_err(|e| e.to_string())?;
    let trees: HashMap<i64, (String, String)> = trees
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut program = bound(db, &format!("EXPLAIN {sql}"), release)?;
    let mut steps = program.raw_query();
    let mut cursors: HashMap<i64, i64> = HashMap::new();
    let mut whole = Vec::new();
    while let Some(row) = steps.next().map_err(|e| e.to_string())? {
        let opcode: String = row.get(1).map_err(|e| e.to_string())?;
        let (cursor, root, database): (i64, i64, i64) = (
            row.get(2).map_err(|e| e.to_string())?,
            row.get(3).map_err(|e| e.to_string())?,
            row.get(4).map_err(|e| e.to_string())?,
        );
        if OPEN.contains(&opcode.as_str()) {
            if database == 0 {
                cursors.insert(cursor, root);
            } else {
                cursors.remove(&cursor);
            }
            continue;
        }
        if !WHOLE.contains(&opcode.as_str()) {
            continue;
        }
        let Some((tree, table)) = cursors.get(&cursor).and_then(|root| trees.get(root)) else {
            continue;
        };
        let at_scale = TABLES
            .iter()
            .find(|t| t.name == table)
            .filter(|t| t.scale.grows_with_objects());
        if let Some(t) = at_scale {
            let through = if tree == table {
                String::new()
            } else {
                format!(" through {tree}")
            };
            let read = format!(
                "{} is read whole{through}, a table {}",
                t.name,
                t.scale.words()
            );
            if !whole.contains(&read) {
                whole.push(read);
            }
        }
    }
    Ok(whole)
}
