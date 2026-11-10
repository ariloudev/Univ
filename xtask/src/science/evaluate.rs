//! Each kind of check evaluated against a loaded release.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::types::ValueRef;

use super::rules::{Check, Exception, Expect, GridPoint, Select};
use super::{Finding, Release, fingerprints, grid, num, objects, pairs};

/// The most rows or pairs shown under a check.
const SHOWN: usize = 12;

/// Evaluates `check` against `release`.
///
/// # Errors
///
/// Fails when what the check selects cannot be read.
pub fn evaluate(release: &Release<'_>, check: &Check) -> Result<Finding, String> {
    let e = &check.expect;
    match &check.select {
        Select::ObjectDistance { object, placed_by } => {
            objects::distance(release, object, placed_by.as_ref(), e)
        }
        Select::ObjectDirection { object } => objects::direction(release, object, e),
        Select::StructureDistance {
            structure,
            velocity_km_s,
            frame,
        } => objects::structure(release, structure, *velocity_km_s, *frame, e),
        Select::GridLabels {
            grid,
            field,
            frame,
            unit,
            points,
        } => grid_labels(release, grid, *field, (frame, unit), points, e),
        Select::Query {
            sql,
            scalar,
            exceptions,
            shown,
        } => query(release, sql, *scalar, exceptions, shown.unwrap_or(SHOWN), e),
        Select::ClosePairs {
            scale_domains,
            kinds,
            radius_arcmin,
            exceptions,
        } => close_pairs(release, scale_domains, kinds, *radius_arcmin, exceptions, e),
        Select::LargestTile {} => largest_tile(release, e),
        Select::D1Estimate {} => {
            // The largest of the release's databases, the catalogue or an object shard.
            let bytes = release
                .manifest
                .estimates()
                .map(|e| e.estimated_bytes)
                .max()
                .unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "bytes far below 2^52")]
            let value = bytes as f64;
            Ok(bounded(value, format!("{bytes} bytes"), e, Vec::new()))
        }
        Select::Fingerprints {} => {
            let (tables, differences) = fingerprints::differences(release.db, release.manifest)?;
            #[expect(clippy::cast_precision_loss, reason = "a handful of differences")]
            let value = differences.len() as f64;
            let found = format!("{} differences over {tables} tables", differences.len());
            Ok(bounded(value, found, e, differences))
        }
        Select::Report { sql } => {
            let (lines, kept, _) = rows(release, sql, &HashSet::new(), usize::MAX)?;
            let mut f = Finding::new(format!("{kept} rows"), "-".into(), true);
            f.details = lines;
            Ok(f)
        }
    }
}

/// A finding whose value is judged by the bounds `equals`, `at_least` and `at_most`.
fn bounded(value: f64, found: String, e: &Expect, details: Vec<String>) -> Finding {
    let mut holds = true;
    let mut expected = Vec::new();
    if let Some(want) = e.equals {
        holds &= (value - want).abs() <= 1e-9 * want.abs().max(1.0);
        expected.push(format!("= {}", num(want)));
    }
    if let Some(least) = e.at_least {
        holds &= value >= least;
        expected.push(format!("≥ {}", num(least)));
    }
    if let Some(most) = e.at_most {
        holds &= value <= most;
        expected.push(format!("≤ {}", num(most)));
    }
    let mut f = Finding::new(found, expected.join(" and "), holds && !value.is_nan());
    f.details = details;
    f
}

/// A value of a row as a line shows it.
fn shown(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(x) => num(x),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("<{} bytes>", b.len()),
    }
}

/// Binds `:release` where the query names it.
fn statement<'c>(release: &Release<'c>, sql: &str) -> Result<rusqlite::Statement<'c>, String> {
    let mut q = release.db.prepare(sql).map_err(|e| e.to_string())?;
    if let Some(i) = q.parameter_index(":release").map_err(|e| e.to_string())? {
        q.raw_bind_parameter(i, release.number)
            .map_err(|e| e.to_string())?;
    }
    Ok(q)
}

/// The rows `sql` gives, those whose first column is an object id in `skip` left out: the
/// first `limit` as lines, how many were kept and how many left out.
fn rows(
    release: &Release<'_>,
    sql: &str,
    skip: &HashSet<i64>,
    limit: usize,
) -> Result<(Vec<String>, usize, usize), String> {
    let mut q = statement(release, sql)?;
    let columns: Vec<String> = q.column_names().map(str::to_owned).collect();
    let mut rows = q.raw_query();
    let (mut lines, mut kept, mut left_out) = (Vec::new(), 0, 0);
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        if let Ok(Some(id)) = row.get::<_, Option<i64>>(0)
            && skip.contains(&id)
        {
            left_out += 1;
            continue;
        }
        kept += 1;
        if lines.len() < limit {
            let mut cells = Vec::new();
            for (i, name) in columns.iter().enumerate() {
                let value = row.get_ref(i).map_err(|e| e.to_string())?;
                cells.push(format!("{name}={}", shown(value)));
            }
            lines.push(cells.join(" "));
        }
    }
    Ok((lines, kept, left_out))
}

/// The object ids `exceptions` name; those the release does not hold are noted.
fn excepted(release: &Release<'_>, exceptions: &[Exception]) -> (HashSet<i64>, Vec<String>) {
    let mut ids = HashSet::new();
    let mut notes = Vec::new();
    for x in exceptions {
        let object = super::rules::ObjectRef::Designation {
            source: x.source.clone(),
            identifier: x.identifier.clone(),
        };
        match objects::resolve(release.db, release.number, &object) {
            Ok(id) => {
                ids.insert(id);
            }
            Err(e) => notes.push(format!("exception {object} not applied: {e}")),
        }
    }
    (ids, notes)
}

fn query(
    release: &Release<'_>,
    sql: &str,
    scalar: bool,
    exceptions: &[Exception],
    shown: usize,
    e: &Expect,
) -> Result<Finding, String> {
    if scalar {
        let mut q = statement(release, sql)?;
        let mut rows = q.raw_query();
        let row = rows
            .next()
            .map_err(|e| e.to_string())?
            .ok_or("the query gave no row")?;
        let value: Option<f64> = row.get(0).map_err(|e| e.to_string())?;
        let value = value.ok_or("the query gave NULL")?;
        return Ok(bounded(value, num(value), e, Vec::new()));
    }
    let (skip, notes) = excepted(release, exceptions);
    let (mut details, kept, left_out) = rows(release, sql, &skip, shown)?;
    #[expect(clippy::cast_precision_loss, reason = "a row count far below 2^52")]
    let value = kept as f64;
    let found = if exceptions.is_empty() {
        format!("{kept} rows")
    } else {
        format!("{kept} rows ({left_out} listed exceptions left out)")
    };
    details.extend(notes);
    Ok(bounded(value, found, e, details))
}

fn close_pairs(
    release: &Release<'_>,
    domains: &[String],
    kinds: &[String],
    radius_arcmin: f64,
    exceptions: &[Exception],
    e: &Expect,
) -> Result<Finding, String> {
    let placed = pairs::placed(release.db, release.number, domains, kinds)?;
    let (skip, notes) = excepted(release, exceptions);
    let found = pairs::close_pairs(&placed, radius_arcmin, &skip);
    let mut details = Vec::new();
    for p in found.iter().take(SHOWN) {
        details.push(format!(
            "{} at {} ± {} Mpc and {} at {} ± {} Mpc, {}′ apart",
            label(release, p.a.id),
            num(p.a.distance),
            num(p.a.sigma),
            label(release, p.b.id),
            num(p.b.distance),
            num(p.b.sigma),
            num(p.arcmin)
        ));
    }
    details.extend(notes);
    #[expect(clippy::cast_precision_loss, reason = "a pair count far below 2^52")]
    let value = found.len() as f64;
    let text = format!(
        "{} pairs among {} placed objects",
        found.len(),
        placed.len()
    );
    Ok(bounded(value, text, e, details))
}

/// An object's id with its entries' sources and names, for a line.
fn label(release: &Release<'_>, id: i64) -> String {
    let entries: Option<String> = release
        .db
        .query_row(
            "SELECT group_concat(s.id || ':' || coalesce(e.name, e.identifier), ', ') FROM \
             entries e JOIN sources s ON s.release = e.release AND s.n = e.source \
             WHERE e.release = ?1 AND e.object_id = ?2",
            rusqlite::params![release.number, id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    format!("{id} [{}]", entries.unwrap_or_default())
}

fn grid_labels(
    release: &Release<'_>,
    id: &str,
    field: usize,
    (frame, unit): (&str, &str),
    points: &[GridPoint],
    e: &Expect,
) -> Result<Finding, String> {
    let g = grid::load(release.dir, release.number, id, field)?;
    if g.header.frame != frame || g.header.unit != unit {
        return Err(format!(
            "grid {id} is in {} {}, the points in {frame} {unit}",
            g.header.frame, g.header.unit
        ));
    }
    let mut wrong = 0usize;
    let mut details = Vec::new();
    for p in points {
        let sample = g.at(p.at);
        #[expect(clippy::cast_possible_truncation, reason = "labels are small integers")]
        let label = sample.map(|s| f64::from(s).round() as i64);
        let name = label.and_then(|l| g.label_name(l)).unwrap_or("-");
        if label != Some(p.label) {
            wrong += 1;
        }
        details.push(format!(
            "{} at {:?}: label {} ({name}), expected {}",
            p.name,
            p.at,
            label.map_or_else(|| "outside the grid".to_owned(), |l| l.to_string()),
            p.label
        ));
    }
    let found = format!("{wrong} of {} points in another label", points.len());
    #[expect(clippy::cast_precision_loss, reason = "a handful of points")]
    let value = wrong as f64;
    let mut f = bounded(value, found, e, details);
    if f.holds {
        f.details.clear();
    }
    Ok(f)
}

fn largest_tile(release: &Release<'_>, e: &Expect) -> Result<Finding, String> {
    let mut largest: Option<(u64, &str)> = None;
    for f in &release.manifest.files {
        let bin = Path::new(&f.path)
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("bin"));
        if !(f.path.starts_with("tiles/") && bin) {
            continue;
        }
        let path = release.dir.join(&f.path);
        let bytes = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();
        if largest.is_none_or(|(b, _)| bytes > b) {
            largest = Some((bytes, &f.path));
        }
    }
    let (bytes, path) = largest.ok_or("the manifest lists no tile")?;
    #[expect(clippy::cast_precision_loss, reason = "bytes far below 2^52")]
    let value = bytes as f64;
    Ok(bounded(
        value,
        format!("{bytes} bytes, {path}"),
        e,
        Vec::new(),
    ))
}
