//! `cargo xtask bench lookup`: object lookups timed through the Worker on the real local state
//! (QA-2: an object lookup within 200 ms at the 95th percentile).
//!
//! It samples [`SAMPLES`] object ids of the latest release at random, starts the Worker under
//! `wrangler dev` on the real state, reads each id once after [`WARM_UP`] reads that are not
//! counted, and prints the 50th, 95th and 99th percentiles. This is the local runtime and its
//! local D1, so it measures the Worker and its queries, not Cloudflare's network.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::integration::{REAL_STATE, migrate, release_tables, start_ready, status};

/// How many lookups are timed.
const SAMPLES: usize = 1_000;

/// How many lookups run first, uncounted, so that the isolate and its caches are warm.
const WARM_UP: usize = 20;

/// QA-2's bound on the 95th percentile.
const P95_BOUND: Duration = Duration::from_millis(200);

/// Times the object lookups; true when every lookup answered 200 within QA-2's bound.
pub fn lookup(root: &Path) -> bool {
    let state = root.join(REAL_STATE);
    let ids = match sample_ids(&state) {
        Ok(ids) => ids,
        Err(error) => {
            eprintln!(
                "could not sample object ids from {}: {error}",
                state.display()
            );
            return false;
        }
    };
    if !release_tables(root, &state) || !migrate(root, &state) {
        return false;
    }
    let log = root.join("target/bench/wrangler.log");
    if let Some(dir) = log.parent()
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        eprintln!("could not create {}: {error}", dir.display());
        return false;
    }
    let Some((wrangler, port)) = start_ready(root, &state, &log) else {
        eprintln!(
            "bench failed at: wrangler dev; its log is {}",
            log.display()
        );
        return false;
    };
    let mut times = Vec::with_capacity(ids.len());
    let mut failures = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let path = format!("/v1/objects/{id}");
        let started = Instant::now();
        let status = status(port, &path, Duration::from_secs(30));
        let took = started.elapsed();
        match status {
            Ok(200) => {}
            Ok(status) => failures.push(format!("{path}: {status}")),
            Err(error) => failures.push(format!("{path}: {error}")),
        }
        if n >= WARM_UP {
            times.push(took);
        }
    }
    drop(wrangler);
    times.sort_unstable();
    let p = |q: f64| percentile(&times, q);
    eprintln!(
        "object lookup over {} ids: p50 {:.1} ms, p95 {:.1} ms, p99 {:.1} ms, max {:.1} ms",
        times.len(),
        ms(p(0.50)),
        ms(p(0.95)),
        ms(p(0.99)),
        ms(times.last().copied().unwrap_or_default()),
    );
    for failure in failures.iter().take(10) {
        eprintln!("not 200: {failure}");
    }
    let within = p(0.95) <= P95_BOUND;
    eprintln!(
        "QA-2 {}: p95 {} the bound of {} ms; {} of {} lookups not 200",
        if within && failures.is_empty() {
            "met"
        } else {
            "NOT met"
        },
        if within { "within" } else { "over" },
        P95_BOUND.as_millis(),
        failures.len(),
        ids.len()
    );
    within && failures.is_empty()
}

/// The `q` quantile of sorted `times`, by the nearest rank.
fn percentile(times: &[Duration], q: f64) -> Duration {
    if times.is_empty() {
        return Duration::ZERO;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank within a list of at most a few thousand times"
    )]
    let rank = ((q * times.len() as f64).ceil() as usize).clamp(1, times.len());
    times[rank - 1]
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

/// [`SAMPLES`] + [`WARM_UP`] object ids of the latest release, at random, read from the
/// release database of the local state before the Worker opens it.
fn sample_ids(state: &Path) -> Result<Vec<u64>, String> {
    let file = release_database(state)?;
    let db =
        rusqlite::Connection::open_with_flags(&file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let count = i64::try_from(SAMPLES + WARM_UP).map_err(|e| e.to_string())?;
    let mut query = db
        .prepare(
            "SELECT id FROM objects WHERE release = (SELECT release FROM latest WHERE one = 1) \
             ORDER BY random() LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let ids = query
        .query_map([count], |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .map(|id| {
            id.map_err(|e| e.to_string())
                .and_then(|id| u64::try_from(id).map_err(|e| e.to_string()))
        })
        .collect::<Result<Vec<u64>, String>>()?;
    if ids.len() < SAMPLES + WARM_UP {
        return Err(format!(
            "the latest release holds only {} objects",
            ids.len()
        ));
    }
    Ok(ids)
}

/// The largest D1 database file of the local state: the release database, which dwarfs the
/// layers database.
fn release_database(state: &Path) -> Result<PathBuf, String> {
    let dir = state.join("v3/d1/miniflare-D1DatabaseObject");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sqlite"))
        .max_by_key(|path| path.metadata().map_or(0, |m| m.len()))
        .ok_or_else(|| format!("{} holds no D1 database", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_95th_percentile_is_the_nearest_rank() {
        let times: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&times, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&times, 0.50), Duration::from_millis(50));
        assert_eq!(percentile(&times[..1], 0.99), Duration::from_millis(1));
        assert_eq!(percentile(&[], 0.95), Duration::ZERO);
    }
}
