//! TAI − UTC, from the IERS tables recorded in `tests/fixtures/iers/` (`Leap_Second.dat`,
//! updated through IERS Bulletin C 72 of July 2026, and `UTC-TAI.history`), which a test holds
//! this table to.
//!
//! From 1961 to 1971 UTC ran with a drift: TAI − UTC = offset + (MJD − base) × rate seconds,
//! MJD being the UTC Modified Julian Date. From 1972 on it is a whole number of seconds, which
//! steps by one at each leap second.

/// The first UTC day the tables give, 1961-01-01, as an MJD.
pub const FIRST_MJD: i64 = 37_300;

/// The day the leap-second table expires, 2027-06-28, as an MJD: UTC from then on cannot be
/// converted, since a leap second might yet be announced before it.
pub const EXPIRES_MJD: i64 = 61_584;

/// One row: from UTC day `from` (an MJD) on, TAI − UTC = `offset` + (MJD − `base`) × `rate`
/// seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub from: i64,
    pub offset: f64,
    pub base: f64,
    pub rate: f64,
}

const fn drift(from: i64, offset: f64, base: f64, rate: f64) -> Row {
    Row {
        from,
        offset,
        base,
        rate,
    }
}

const fn leap(from: i64, seconds: f64) -> Row {
    Row {
        from,
        offset: seconds,
        base: 0.0,
        rate: 0.0,
    }
}

/// Every row, oldest first.
pub const TABLE: [Row; 41] = [
    drift(37_300, 1.422_818_0, 37_300.0, 0.001_296),
    drift(37_512, 1.372_818_0, 37_300.0, 0.001_296),
    drift(37_665, 1.845_858_0, 37_665.0, 0.001_123_2),
    drift(38_334, 1.945_858_0, 37_665.0, 0.001_123_2),
    drift(38_395, 3.240_130_0, 38_761.0, 0.001_296),
    drift(38_486, 3.340_130_0, 38_761.0, 0.001_296),
    drift(38_639, 3.440_130_0, 38_761.0, 0.001_296),
    drift(38_761, 3.540_130_0, 38_761.0, 0.001_296),
    drift(38_820, 3.640_130_0, 38_761.0, 0.001_296),
    drift(38_942, 3.740_130_0, 38_761.0, 0.001_296),
    drift(39_004, 3.840_130_0, 38_761.0, 0.001_296),
    drift(39_126, 4.313_170_0, 39_126.0, 0.002_592),
    drift(39_887, 4.213_170_0, 39_126.0, 0.002_592),
    leap(41_317, 10.0),
    leap(41_499, 11.0),
    leap(41_683, 12.0),
    leap(42_048, 13.0),
    leap(42_413, 14.0),
    leap(42_778, 15.0),
    leap(43_144, 16.0),
    leap(43_509, 17.0),
    leap(43_874, 18.0),
    leap(44_239, 19.0),
    leap(44_786, 20.0),
    leap(45_151, 21.0),
    leap(45_516, 22.0),
    leap(46_247, 23.0),
    leap(47_161, 24.0),
    leap(47_892, 25.0),
    leap(48_257, 26.0),
    leap(48_804, 27.0),
    leap(49_169, 28.0),
    leap(49_534, 29.0),
    leap(50_083, 30.0),
    leap(50_630, 31.0),
    leap(51_179, 32.0),
    leap(53_736, 33.0),
    leap(54_832, 34.0),
    leap(56_109, 35.0),
    leap(57_204, 36.0),
    leap(57_754, 37.0),
];

/// The row in force on UTC day `day`, an MJD; none before 1961 or from the table's expiry on.
#[must_use]
pub fn row(day: i64) -> Option<&'static Row> {
    if !(FIRST_MJD..EXPIRES_MJD).contains(&day) {
        return None;
    }
    TABLE.iter().rev().find(|row| row.from <= day)
}

/// TAI − UTC in seconds at UTC MJD `mjd`, under `row`.
#[must_use]
pub fn tai_minus_utc(row: &Row, mjd: f64) -> f64 {
    row.offset + (mjd - row.base) * row.rate
}

/// Whether UTC day `day` ends with a leap second, a 61-second last minute: the next row is a
/// whole second more and starts the next day.
#[must_use]
pub fn ends_with_leap_second(day: i64) -> bool {
    TABLE
        .windows(2)
        .any(|pair| pair[1].from == day + 1 && pair[1].rate == 0.0 && pair[0].rate == 0.0)
}
