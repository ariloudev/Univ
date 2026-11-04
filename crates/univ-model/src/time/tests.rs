use super::*;

fn record(timescale: Timescale, at: At, uncertainty: TimeUncertainty) -> TimeRecord {
    TimeRecord {
        kind: TimeKind::Arrival,
        timescale,
        at,
        uncertainty,
    }
}

fn instant(text: &str, timescale: Timescale) -> Result<f64, TimeError> {
    let at = At::Instant {
        instant: text.to_owned(),
    };
    record(timescale, at, TimeUncertainty::NoneReported)
        .on_tt()
        .map(|on| on.at)
}

const SECOND: f64 = 1.0 / 86_400.0;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * SECOND
}

#[test]
fn j2000_is_day_zero_on_tt() {
    assert_eq!(instant("2000-01-01T12:00:00", Timescale::Tt), Ok(0.0));
    assert_eq!(days_from_civil(1972, 1, 1) + 40_587, 41_317);
}

#[test]
fn utc_in_2000_is_tt_less_32_leap_seconds_and_32_184_s() {
    let tt = instant("2000-01-01T11:58:55.816Z", Timescale::Utc).unwrap();
    assert!(close(tt, 0.0), "{}", tt / SECOND);
}

#[test]
fn a_leap_second_is_the_second_before_the_next_days_first() {
    let leap = instant("2016-12-31T23:59:60Z", Timescale::Utc).unwrap();
    let next = instant("2017-01-01T00:00:00Z", Timescale::Utc).unwrap();
    assert!(close(next - leap, SECOND), "{}", (next - leap) / SECOND);
    let before = instant("2016-12-31T23:59:59Z", Timescale::Utc).unwrap();
    assert!(close(leap - before, SECOND));
    for (text, timescale) in [
        ("2016-12-30T23:59:60Z", Timescale::Utc),
        ("2016-12-31T23:58:60Z", Timescale::Utc),
        ("2016-12-31T23:59:60", Timescale::Tt),
    ] {
        assert!(instant(text, timescale).is_err(), "{text}");
    }
}

#[test]
fn utc_from_1961_to_1971_drifts_as_the_table_says() {
    let start = instant("1961-01-01T00:00:00Z", Timescale::Utc).unwrap();
    let mjd = 37_300.0 - J2000_MJD;
    assert!(close(start - mjd, (1.422_818 + TT_MINUS_TAI) * SECOND));
    let later = instant("1967-06-07T00:00:00Z", Timescale::Utc).unwrap();
    let day = f64::from(u16::try_from(days_from_civil(1967, 6, 7) + 40_587).unwrap());
    let tai_utc = 4.313_17 + (day - 39_126.0) * 0.002_592;
    assert!(close(
        later - (day - J2000_MJD),
        (tai_utc + TT_MINUS_TAI) * SECOND
    ));
}

#[test]
fn utc_before_1961_or_from_the_tables_expiry_is_refused_and_tt_is_not() {
    for text in [
        "1960-12-31T23:59:59Z",
        "2027-06-28T00:00:00Z",
        // Tycho's supernova, first seen 1572-11-11 in the Julian calendar, is 1572-11-21 in
        // the proleptic Gregorian calendar instants are read in; it predates UTC either way.
        "1572-11-21T00:00:00Z",
    ] {
        let error = instant(text, Timescale::Utc).unwrap_err();
        assert_eq!(error.member(), "instant");
        assert!(error.to_string().contains("give the time in TT"), "{error}");
    }
    assert!(instant("2027-06-27T23:59:59Z", Timescale::Utc).is_ok());
    assert!(instant("1572-11-21T00:00:00", Timescale::Tt).is_ok());
    assert!(instant("2100-01-01T00:00:00", Timescale::Tt).is_ok());
}

#[test]
fn a_malformed_instant_is_refused() {
    for text in [
        "1987-2-23T07:35:35",
        "0000-01-01T00:00:00",
        "1987-02-29T00:00:00",
        "1987-02-23",
        "1987-02-23T07:35",
        "1987-02-23T07:35:35+01:00",
        "1987-02-23T24:00:00",
        "1987-02-23T07:35:35.",
        "1987-02-23T07:35:3a",
    ] {
        assert!(instant(text, Timescale::Tt).is_err(), "{text}");
    }
    assert!(
        instant("1987-02-23T07:35:35Z", Timescale::Tt).is_err(),
        "Z marks UTC"
    );
    assert!(instant("2000-02-29T00:00:00.125", Timescale::Tt).is_ok());
}

#[test]
fn a_span_before_j2000_is_on_tt_in_julian_years() {
    let ago = At::YearsBefore {
        years_before_j2000: 1.3e8,
    };
    let interval = TimeUncertainty::Interval {
        minus: 2e7,
        plus: 1e7,
        unit: TimeUnit::JulianYear,
    };
    let on = record(Timescale::Tt, ago.clone(), interval)
        .on_tt()
        .unwrap();
    assert_eq!(on.at, -1.3e8 * 365.25);
    assert_eq!(on.earliest, -1.5e8 * 365.25);
    assert_eq!(on.latest, -1.2e8 * 365.25);
    let error = record(Timescale::Utc, ago, interval).on_tt().unwrap_err();
    assert_eq!(error.member(), "years_before_j2000");
}

#[test]
fn an_uncertainty_is_an_interval_in_its_own_unit_and_never_negative() {
    let at = At::Instant {
        instant: "2000-01-01T12:00:00".into(),
    };
    let sigma = TimeUncertainty::Symmetric {
        sigma: 30.0,
        unit: TimeUnit::Second,
    };
    let on = record(Timescale::Tt, at.clone(), sigma).on_tt().unwrap();
    assert!(close(on.earliest, -30.0 * SECOND) && close(on.latest, 30.0 * SECOND));
    let none = record(Timescale::Tt, at.clone(), TimeUncertainty::NoneReported);
    let on = none.on_tt().unwrap();
    assert_eq!((on.earliest, on.latest), (0.0, 0.0));
    let negative = TimeUncertainty::Interval {
        minus: -1.0,
        plus: 1.0,
        unit: TimeUnit::Day,
    };
    let error = record(Timescale::Tt, at, negative).on_tt().unwrap_err();
    assert_eq!(error.member(), "uncertainty");
}

#[test]
fn a_record_reads_from_json_in_either_form() {
    let arrival: TimeRecord = serde_json::from_str(
        r#"{"kind":"arrival","timescale":"utc","instant":"1987-02-23T07:35:35Z",
            "uncertainty":{"form":"symmetric","sigma":60,"unit":"s"}}"#,
    )
    .unwrap();
    assert!(matches!(arrival.at, At::Instant { .. }));
    let source: TimeRecord = serde_json::from_str(
        r#"{"kind":"source_event","timescale":"tt","years_before_j2000":1.3e8,
            "uncertainty":{"form":"none_reported"}}"#,
    )
    .unwrap();
    assert!(matches!(source.at, At::YearsBefore { .. }));
}
