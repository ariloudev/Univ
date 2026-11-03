//! The Worker reads the database and bucket Univ Ingest publishes to: local D1 names a
//! database's file after its `database_id`, so the Worker's `wrangler.toml` must give the
//! release database the same name and id as the root `wrangler.toml`, and the same bucket, or
//! it opens an empty store and says nothing.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::path::Path;

/// The value of each `key = "value"` line of the TOML table that holds `marker`, a line such
/// as `database_name = "univ"`; tables are told apart by their `[[...]]` headers.
fn table_with<'a>(toml: &'a str, marker: &str) -> Vec<(&'a str, &'a str)> {
    let mut table = Vec::new();
    let mut found = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            if found {
                break;
            }
            table.clear();
            continue;
        }
        found |= line == marker;
        if let Some((key, value)) = line.split_once('=') {
            table.push((key.trim(), value.trim().trim_matches('"')));
        }
    }
    assert!(found, "no table holds {marker}");
    table
}

fn value<'a>(table: &[(&'a str, &'a str)], key: &str) -> &'a str {
    table.iter().find(|(k, _)| *k == key).unwrap().1
}

fn read(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(path)).unwrap()
}

#[test]
fn the_worker_reads_the_database_and_bucket_that_ingest_publishes_to() {
    let ingest = read("wrangler.toml");
    let worker = read("crates/univ-server/wrangler.toml");
    let marker = "database_name = \"univ\"";
    let (published, served) = (table_with(&ingest, marker), table_with(&worker, marker));
    assert_eq!(value(&served, "binding"), "DB");
    assert_eq!(
        value(&served, "database_id"),
        value(&published, "database_id")
    );
    let bucket = "bucket_name = \"univ-releases\"";
    assert_eq!(value(&table_with(&ingest, bucket), "binding"), "RELEASES");
    assert_eq!(value(&table_with(&worker, bucket), "binding"), "RELEASES");
}

#[test]
fn the_layer_database_is_another_file_than_the_release_database() {
    let worker = read("crates/univ-server/wrangler.toml");
    let releases = table_with(&worker, "database_name = \"univ\"");
    let layers = table_with(&worker, "database_name = \"univ-layers\"");
    assert_eq!(value(&layers, "binding"), "LAYERS");
    assert_ne!(
        value(&layers, "database_id"),
        value(&releases, "database_id")
    );
}

#[test]
fn each_rate_limit_binding_allows_what_its_tier_allows_in_the_same_window() {
    use univ_api::Limiter;
    use univ_model::api::keys::{RATE_WINDOW_SECONDS, RateLimitTier};
    let worker = read("crates/univ-server/wrangler.toml");
    for (name, limiter) in [
        ("ANON", Limiter::Anonymous),
        ("BASIC", Limiter::Key(RateLimitTier::Basic)),
        ("STANDARD", Limiter::Key(RateLimitTier::Standard)),
        ("EXTENDED", Limiter::Key(RateLimitTier::Extended)),
    ] {
        let table = table_with(&worker, &format!("name = \"{name}\""));
        let simple = value(&table, "simple");
        assert_eq!(
            simple,
            format!(
                "{{ limit = {}, period = {RATE_WINDOW_SECONDS} }}",
                limiter.requests()
            ),
            "{name}"
        );
    }
}
