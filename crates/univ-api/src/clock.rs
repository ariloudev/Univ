//! Times as Univ Server writes them: RFC 3339 in UTC, to the millisecond.

/// `millis` since the Unix epoch as RFC 3339 UTC, such as `2026-11-03T12:00:00.000Z`.
#[must_use]
pub fn rfc3339(millis: u64) -> String {
    let seconds = millis / 1_000;
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3_600,
        of_day % 3_600 / 60,
        of_day % 60,
        millis % 1_000
    )
}

/// The proleptic Gregorian date of `days` since 1970-01-01 (Hinnant, "chrono-Compatible
/// Low-Level Date Algorithms", `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(0),
        u32::try_from(day).unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    #[test]
    fn times_are_rfc_3339_utc_to_the_millisecond() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(rfc3339(1_793_707_200_123), "2026-11-03T12:00:00.123Z");
        assert_eq!(rfc3339(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
    }
}
