//! Latency benches through the Worker on the real local state, each against its quality
//! attribute:
//!
//! - `cargo xtask bench lookup`: object lookups (QA-2, within 200 ms at the 95th percentile).
//!   [`SAMPLES`] object ids of the latest release, at random, each read once.
//! - `cargo xtask bench volume`: the first page of a bounded volume query (QA-3, within 500 ms
//!   at the 95th percentile). [`VOLUMES`] spheres of 50 Mpc radius centred 100 Mpc from the
//!   barycentre in directions spread over the sky, each queried once.
//!
//! Each starts the Worker under `wrangler dev` on the real state, makes [`WARM_UP`] requests
//! that are not counted, and prints the 50th, 95th and 99th percentiles. This is the local
//! runtime and its local D1, so it measures the Worker and its queries, not Cloudflare's
//! network.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::integration::{Exchange, REAL_STATE, send};
use crate::owner::{real_owner, with_real_server};

/// How many lookups are timed.
const SAMPLES: usize = 1_000;

/// How many volume queries are timed.
const VOLUMES: usize = 200;

/// How many requests run first, uncounted, so that the isolate and its caches are warm.
const WARM_UP: usize = 20;

/// QA-2's bound on the 95th percentile.
const LOOKUP_P95: Duration = Duration::from_millis(200);

/// QA-3's bound on the 95th percentile.
const VOLUME_P95: Duration = Duration::from_millis(500);

/// A request to time: its method, path and JSON body.
struct Timed {
    method: &'static str,
    path: String,
    json: Option<String>,
}

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
    let requests: Vec<Timed> = ids
        .iter()
        .map(|id| Timed {
            method: "GET",
            path: format!("/v1/objects/{id}"),
            json: None,
        })
        .collect();
    bench(root, "object lookup", "QA-2", &requests, LOOKUP_P95)
}

/// Times the first pages of volume queries; true when every one answered 200 within QA-3's
/// bound.
pub fn volume(root: &Path) -> bool {
    let requests: Vec<Timed> = centres(VOLUMES + WARM_UP, 100.0)
        .into_iter()
        .map(|[x, y, z]| Timed {
            method: "POST",
            path: "/v1/query/volume".to_owned(),
            json: Some(format!(
                r#"{{"volume":{{"frame":"icrs","unit":"megaparsec","form":"sphere","centre":[{x},{y},{z}],"radius":50}}}}"#
            )),
        })
        .collect();
    bench(
        root,
        "volume query, first page",
        "QA-3",
        &requests,
        VOLUME_P95,
    )
}

/// `n` points `distance` from the origin, spread over the sphere: a Fibonacci lattice, so the
/// run is the same each time.
fn centres(n: usize, distance: f64) -> Vec<[f64; 3]> {
    let golden = f64::midpoint(1.0, 5.0_f64.sqrt());
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    let count = f64::from(n);
    (0..n)
        .map(|i| {
            let k = f64::from(i) + 0.5;
            let z = 1.0 - 2.0 * k / count;
            let r = (1.0 - z * z).sqrt();
            let phi = std::f64::consts::TAU * k / golden;
            [
                distance * r * phi.cos(),
                distance * r * phi.sin(),
                distance * z,
            ]
        })
        .collect()
}

/// Starts the Worker, times `requests` after the first [`WARM_UP`], and reports them against
/// `bound`; true when all answered 200 and the 95th percentile is within it.
fn bench(root: &Path, what: &str, attribute: &str, requests: &[Timed], bound: Duration) -> bool {
    let Some(owner) = real_owner(root) else {
        return false;
    };
    let log = root.join("target/bench/wrangler.log");
    with_real_server(root, &owner, &log, "bench", |port, key| {
        let (times, failures) = time(port, key, requests);
        report(what, attribute, &times, &failures, bound, requests.len())
    })
}

/// Each request's time after the first [`WARM_UP`], and the requests not answered 200.
fn time(port: u16, key: &str, requests: &[Timed]) -> (Vec<Duration>, Vec<String>) {
    let mut times = Vec::with_capacity(requests.len());
    let mut failures = Vec::new();
    for (n, request) in requests.iter().enumerate() {
        let exchange = Exchange {
            method: request.method,
            path: &request.path,
            bearer: Some(key),
            json: request.json.as_deref(),
        };
        let started = Instant::now();
        let status = send(port, &exchange, Duration::from_secs(30)).map(|(status, _)| status);
        let took = started.elapsed();
        match status {
            Ok(200) => {}
            Ok(status) => failures.push(format!("{} {}: {status}", request.method, request.path)),
            Err(error) => failures.push(format!("{} {}: {error}", request.method, request.path)),
        }
        if n >= WARM_UP {
            times.push(took);
        }
    }
    times.sort_unstable();
    (times, failures)
}

/// Prints the percentiles and whether the quality attribute was met.
fn report(
    what: &str,
    attribute: &str,
    times: &[Duration],
    failures: &[String],
    bound: Duration,
    total: usize,
) -> bool {
    let p = |q: f64| percentile(times, q);
    eprintln!(
        "{what} over {} requests: p50 {:.1} ms, p95 {:.1} ms, p99 {:.1} ms, max {:.1} ms",
        times.len(),
        ms(p(0.50)),
        ms(p(0.95)),
        ms(p(0.99)),
        ms(times.last().copied().unwrap_or_default()),
    );
    for failure in failures.iter().take(10) {
        eprintln!("not 200: {failure}");
    }
    let within = p(0.95) <= bound;
    eprintln!(
        "{attribute} {}: p95 {} the bound of {} ms; {} of {total} requests not 200",
        if within && failures.is_empty() {
            "met"
        } else {
            "NOT met"
        },
        if within { "within" } else { "over" },
        bound.as_millis(),
        failures.len(),
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
/// catalogue database of the local state, which names every object's shard, before the Worker
/// opens it.
fn sample_ids(state: &Path) -> Result<Vec<u64>, String> {
    let db = catalogue_database(state)?;
    let count = i64::try_from(SAMPLES + WARM_UP).map_err(|e| e.to_string())?;
    let mut query = db
        .prepare(
            "SELECT id FROM object_shards WHERE release = \
             (SELECT release FROM latest WHERE one = 1) ORDER BY random() LIMIT ?1",
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

/// The catalogue database of the local state, opened to read: of its D1 database files, the
/// one whose `latest` points to a release. Wrangler names each file by a hash, and an object
/// shard holds the same tables, empty.
fn catalogue_database(state: &Path) -> Result<rusqlite::Connection, String> {
    let dir = state.join("v3/d1/miniflare-D1DatabaseObject");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sqlite"))
        .collect();
    for file in files {
        let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
        let Ok(db) = rusqlite::Connection::open_with_flags(&file, flags) else {
            continue;
        };
        let latest = db.query_row("SELECT release FROM latest WHERE one = 1", [], |row| {
            row.get::<_, i64>(0)
        });
        if latest.is_ok() {
            return Ok(db);
        }
    }
    Err(format!(
        "no D1 database of {} has a latest release: publish one first",
        dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_centres_lie_100_mpc_out_in_both_hemispheres() {
        let points = centres(200, 100.0);
        assert!(
            points
                .iter()
                .all(|p| (p.iter().map(|c| c * c).sum::<f64>().sqrt() - 100.0).abs() < 1e-9)
        );
        assert!(points.iter().any(|p| p[2] > 90.0) && points.iter().any(|p| p[2] < -90.0));
    }

    #[test]
    fn the_95th_percentile_is_the_nearest_rank() {
        let times: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&times, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&times, 0.50), Duration::from_millis(50));
        assert_eq!(percentile(&times[..1], 0.99), Duration::from_millis(1));
        assert_eq!(percentile(&[], 0.95), Duration::ZERO);
    }
}
