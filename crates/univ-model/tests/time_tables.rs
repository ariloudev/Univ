//! The TAI − UTC table of `univ_model::time::utc` against the IERS files it was taken from,
//! recorded in `tests/fixtures/iers/`.
#![expect(
    clippy::unwrap_used,
    reason = "a test unwraps: a failure is the test failing"
)]

use univ_model::time::utc::{EXPIRES_MJD, FIRST_MJD, TABLE};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/iers")
            .join(name),
    )
    .unwrap()
}

/// The MJD of a Gregorian date from 1858-11-17 on.
fn mjd(year: i64, month: i64, day: i64) -> i64 {
    let (y, m) = if month <= 2 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    365 * y + y / 4 - y / 100 + y / 400 + (306_001 * (m + 1)) / 10_000 + day - 679_004
}

#[test]
fn every_leap_second_row_and_the_expiry_are_the_bulletin_cs() {
    let file = fixture("Leap_Second.dat");
    let rows: Vec<(i64, f64)> = file
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let day = fields[0].parse::<f64>().unwrap();
            #[expect(clippy::cast_possible_truncation, reason = "an MJD is whole")]
            (day as i64, fields[4].parse().unwrap())
        })
        .collect();
    let leaps: Vec<(i64, f64)> = TABLE
        .iter()
        .filter(|row| row.rate == 0.0)
        .map(|row| (row.from, row.offset))
        .collect();
    assert_eq!(leaps, rows);
    let expiry = file
        .lines()
        .find_map(|line| line.split("File expires on ").nth(1))
        .unwrap()
        .trim();
    assert_eq!(expiry, "28 June 2027");
    assert_eq!(EXPIRES_MJD, mjd(2027, 6, 28));
}

#[test]
fn every_row_of_1961_to_1971_is_the_historys() {
    let history = fixture("UTC-TAI.history");
    // Each drift row's offset, as the history writes it, and the base and rate it names.
    let offsets: Vec<f64> = history
        .lines()
        .filter_map(|line| {
            // The offset is the number after the last wide gap before "s +", its digits
            // grouped by single spaces.
            let (before, _) = line.split_once("s +")?;
            let offset = before.rsplit("   ").next()?;
            offset.replace(' ', "").parse().ok()
        })
        .collect();
    let drifts: Vec<f64> = TABLE
        .iter()
        .filter(|r| r.rate != 0.0)
        .map(|r| r.offset)
        .collect();
    assert_eq!(offsets, drifts);
    let bases: [(f64, f64); 4] = [
        (37_300.0, 0.001_296),
        (37_665.0, 0.001_123_2),
        (38_761.0, 0.001_296),
        (39_126.0, 0.002_592),
    ];
    for (base, rate) in bases {
        let text = format!("(MJD - {}", f64_text(base));
        assert!(history.contains(&text), "{text}");
        assert!(
            TABLE
                .iter()
                .any(|r| r.base.to_bits() == base.to_bits() && r.rate.to_bits() == rate.to_bits())
        );
    }
    let starts = [
        (1961, 1, 1),
        (1961, 8, 1),
        (1962, 1, 1),
        (1963, 11, 1),
        (1964, 1, 1),
        (1964, 4, 1),
        (1964, 9, 1),
        (1965, 1, 1),
        (1965, 3, 1),
        (1965, 7, 1),
        (1965, 9, 1),
        (1966, 1, 1),
        (1968, 2, 1),
    ];
    let froms: Vec<i64> = TABLE
        .iter()
        .filter(|r| r.rate != 0.0)
        .map(|r| r.from)
        .collect();
    let expected: Vec<i64> = starts.iter().map(|(y, m, d)| mjd(*y, *m, *d)).collect();
    assert_eq!(froms, expected);
    assert_eq!(FIRST_MJD, mjd(1961, 1, 1));
}

/// A base MJD as the history writes it, such as `37 300`.
fn f64_text(base: f64) -> String {
    #[expect(clippy::cast_possible_truncation, reason = "a base MJD is whole")]
    let whole = base as i64;
    format!("{} {:03}", whole / 1_000, whole % 1_000)
}
