//! `cargo xtask science <release-dir> [--db <file>] [--rules <file>]`: loads a built
//! release's D1 rows into a scratch `SQLite` database and runs every check of
//! `rules/science-checks.yaml` against it, printing one line per check and a summary. Exits
//! non-zero on any FAIL.
//!
//! The database is in memory unless `--db` names a file; a file that exists already is taken
//! as loaded, so that checks can be rerun without loading again.

mod evaluate;
mod fingerprints;
mod grid;
mod load;
mod objects;
mod pairs;
mod rules;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::Connection;
use univ_model::release::{ReleaseManifest, keys};

use rules::Severity;

/// A release loaded for checking.
pub struct Release<'a> {
    pub db: &'a Connection,
    pub dir: &'a Path,
    pub manifest: &'a ReleaseManifest,
    pub number: u32,
}

/// What a check found.
pub struct Finding {
    pub found: String,
    pub expected: String,
    pub holds: bool,
    /// Lines shown under a check that does not hold, or a report.
    pub details: Vec<String>,
}

impl Finding {
    fn new(found: String, expected: String, holds: bool) -> Self {
        Self {
            found,
            expected,
            holds,
            details: Vec::new(),
        }
    }
}

/// A number as a line shows it: an integer whole, else to four significant digits or so.
fn num(x: f64) -> String {
    if !x.is_finite() {
        return format!("{x}");
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{x:.0}");
    }
    let digits = if x.abs() >= 100.0 {
        2
    } else if x.abs() >= 1.0 {
        3
    } else {
        4
    };
    let shown = format!("{x:.digits$}");
    if shown
        .trim_start_matches('-')
        .trim_start_matches(['0', '.'])
        .is_empty()
    {
        format!("{x:.2e}")
    } else {
        shown
    }
}

/// The options of the command.
struct Options {
    release: PathBuf,
    db: Option<PathBuf>,
    rules: PathBuf,
}

fn options(root: &Path, args: &[String]) -> Result<Options, String> {
    let mut release = None;
    let mut db = None;
    let mut rules = root.join("rules/science-checks.yaml");
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next().map(PathBuf::from),
            "--rules" => {
                rules = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--rules takes a file")?;
            }
            other if release.is_none() && !other.starts_with("--") => {
                release = Some(PathBuf::from(other));
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    Ok(Options {
        release: release.ok_or("usage: cargo xtask science <release-dir> [--db <file>]")?,
        db,
        rules,
    })
}

/// Runs the command; whether no check failed.
pub fn run(root: &Path, args: &[String]) -> bool {
    match science(root, args, &mut std::io::stdout().lock()) {
        Ok(passed) => passed,
        Err(error) => {
            eprintln!("science: {error}");
            false
        }
    }
}

/// Loads the release and runs the checks, writing the lines to `out`; whether no check failed.
///
/// # Errors
///
/// Fails when the manifest, the checks or the release's D1 files cannot be read or loaded.
fn science(root: &Path, args: &[String], out: &mut dyn std::io::Write) -> Result<bool, String> {
    let options = options(root, args)?;
    let checks = rules::read(&options.rules)?;
    let path = options.release.join(keys::MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: ReleaseManifest =
        serde_norway::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(parent) = manifest.parent {
        return Err(format!(
            "release {} holds only what changed from release {parent}, so its D1 files alone \
             are not the release: the science checks read a release written whole",
            manifest.release
        ));
    }
    let (db, loaded) = load::open(options.db.as_deref())?;
    let started = Instant::now();
    let load_line = if loaded {
        "load: skipped, the database was loaded already".to_owned()
    } else {
        let files = load::load(&db, &options.release, &manifest)?;
        format!(
            "load: {files} D1 files of release {} in {:.1} s",
            manifest.release,
            started.elapsed().as_secs_f64()
        )
    };
    let write = |out: &mut dyn std::io::Write, line: &str| {
        writeln!(out, "{line}").map_err(|e| format!("cannot write: {e}"))
    };
    write(out, &load_line)?;
    load::register(&db, manifest.release, &manifest.cosmology)?;
    let release = Release {
        db: &db,
        dir: &options.release,
        manifest: &manifest,
        number: manifest.release,
    };
    let started = Instant::now();
    let mut counts = [0usize; 4];
    for check in &checks.checks {
        let begun = Instant::now();
        let (status, finding) = match evaluate::evaluate(&release, check) {
            Ok(f) if check.severity == Severity::Report => ("INFO", f),
            Ok(f) if f.holds => ("PASS", f),
            Ok(f) if check.severity == Severity::Concern => ("CONCERN", f),
            Ok(f) => ("FAIL", f),
            Err(e) => (
                "FAIL",
                Finding::new(format!("error: {e}"), "-".into(), false),
            ),
        };
        let slot = ["PASS", "FAIL", "CONCERN", "INFO"]
            .iter()
            .position(|s| *s == status)
            .unwrap_or(1);
        counts[slot] += 1;
        let cite = check
            .cite
            .iter()
            .map(|c| match &c.locator {
                Some(l) => format!("{}, {l}", c.paper),
                None => c.paper.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ");
        write(
            out,
            &format!(
                "{status:<7} {} ({}): found {}; expected {} [{cite}] ({:.1} s)",
                check.id,
                check.title,
                finding.found,
                finding.expected,
                begun.elapsed().as_secs_f64()
            ),
        )?;
        if status != "PASS" {
            if let Some(note) = check.note.as_deref().filter(|_| status != "INFO") {
                write(out, &format!("          note: {}", note.trim()))?;
            }
            for line in &finding.details {
                write(out, &format!("          {line}"))?;
            }
        }
    }
    write(
        out,
        &format!(
            "science: release {}, {} checks: {} PASS, {} FAIL, {} CONCERN, {} INFO; checks took {:.1} s",
            manifest.release,
            checks.checks.len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            started.elapsed().as_secs_f64()
        ),
    )?;
    Ok(counts[1] == 0)
}
