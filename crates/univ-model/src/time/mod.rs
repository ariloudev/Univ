//! The time records of layer items (publish-a-layer-item, filter-by-time): when an event
//! happened at its source, when its signal arrived, or when it was discovered.
//!
//! A time is a calendar instant in UTC or TT, ISO 8601 for years 1 to 9999 in the proleptic
//! Gregorian calendar (a date of the Julian calendar, such as most before 1582, is converted
//! by the client first), or for the deep past a span of Julian years before J2000.0 on TT. Each is turned into TT days from J2000.0
//! (2000-01-01T12:00:00 TT), with its uncertainty as an interval, which is what queries
//! compare. TT = TAI + 32.184 s exactly (IAU 1991 Resolution A4), and TAI − UTC comes from the
//! IERS tables of [`utc`]: UTC is converted from 1961-01-01, when it began, until the
//! leap-second table expires.

pub mod utc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// TT − TAI, in seconds.
pub const TT_MINUS_TAI: f64 = 32.184;

/// The MJD of J2000.0, 2000-01-01T12:00:00 TT.
pub const J2000_MJD: f64 = 51_544.5;

/// The days of a Julian year.
pub const JULIAN_YEAR_DAYS: f64 = 365.25;

/// What a time record dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimeKind {
    /// When the event happened at its source.
    SourceEvent,
    /// When its signal reached the observer.
    Arrival,
    /// When it was discovered.
    Discovery,
}

/// The timescale a time is given in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Timescale {
    Utc,
    Tt,
}

/// The unit of an uncertainty: seconds, days, or Julian years of 365.25 days.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TimeUnit {
    #[serde(rename = "s")]
    Second,
    #[serde(rename = "d")]
    Day,
    #[serde(rename = "a")]
    JulianYear,
}

impl TimeUnit {
    /// Its length in days.
    #[must_use]
    pub fn days(self) -> f64 {
        match self {
            Self::Second => 1.0 / 86_400.0,
            Self::Day => 1.0,
            Self::JulianYear => JULIAN_YEAR_DAYS,
        }
    }
}

/// A time record's uncertainty.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "form", deny_unknown_fields)]
pub enum TimeUncertainty {
    /// No uncertainty was reported: not the same as none at all.
    NoneReported,
    /// ±`sigma`, one standard deviation.
    Symmetric { sigma: f64, unit: TimeUnit },
    /// The time lies between `minus` before it and `plus` after it.
    Interval {
        minus: f64,
        plus: f64,
        unit: TimeUnit,
    },
}

/// When a time record's time is: an instant, or a span before J2000.0.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum At {
    /// An ISO 8601 instant in the record's timescale, proleptic Gregorian, such as
    /// `1987-02-23T07:35:35Z` in UTC or `2017-08-17T12:42:09.184` in TT; a trailing `Z` is
    /// allowed in UTC alone.
    Instant { instant: String },
    /// Julian years before J2000.0, on TT, for times before year 1 such as a distant source
    /// event.
    YearsBefore { years_before_j2000: f64 },
}

/// A time attached to an item, with its kind, its timescale and its uncertainty.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeRecord {
    pub kind: TimeKind,
    pub timescale: Timescale,
    #[serde(flatten)]
    pub at: At,
    pub uncertainty: TimeUncertainty,
}

/// A time record on TT, in days from J2000.0: its time and its uncertainty interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnTt {
    pub at: f64,
    pub earliest: f64,
    pub latest: f64,
}

/// Why a time record cannot be placed on TT, with the member at fault.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TimeError {
    #[error("{0}")]
    Instant(String),
    #[error("{0}")]
    YearsBefore(&'static str),
    #[error("{0}")]
    Uncertainty(&'static str),
}

impl TimeError {
    /// The member of the time record at fault.
    #[must_use]
    pub fn member(&self) -> &'static str {
        match self {
            Self::Instant(_) => "instant",
            Self::YearsBefore(_) => "years_before_j2000",
            Self::Uncertainty(_) => "uncertainty",
        }
    }
}

impl TimeRecord {
    /// The record on TT: its time and uncertainty interval in days from J2000.0.
    ///
    /// # Errors
    ///
    /// A malformed instant, a UTC instant before 1961 or after the leap-second table expires,
    /// a span before J2000.0 given in UTC, or an uncertainty that is negative or not finite.
    pub fn on_tt(&self) -> Result<OnTt, TimeError> {
        let at = self.at.on_tt(self.timescale)?;
        let (minus, plus) = match self.uncertainty {
            TimeUncertainty::NoneReported => (0.0, 0.0),
            TimeUncertainty::Symmetric { sigma, unit } => {
                (sigma * unit.days(), sigma * unit.days())
            }
            TimeUncertainty::Interval { minus, plus, unit } => {
                (minus * unit.days(), plus * unit.days())
            }
        };
        if !(minus.is_finite() && plus.is_finite() && minus >= 0.0 && plus >= 0.0) {
            return Err(TimeError::Uncertainty(
                "an uncertainty is a finite number, 0 or more",
            ));
        }
        Ok(OnTt {
            at,
            earliest: at - minus,
            latest: at + plus,
        })
    }
}

impl At {
    /// The time in `timescale` as TT days from J2000.0.
    ///
    /// # Errors
    ///
    /// A malformed instant, a UTC instant before 1961 or after the leap-second table expires,
    /// or a span before J2000.0 given in UTC or not a finite number, 0 or more.
    pub fn on_tt(&self, timescale: Timescale) -> Result<f64, TimeError> {
        match self {
            Self::Instant { instant } => instant_on_tt(instant, timescale),
            Self::YearsBefore { years_before_j2000 } => {
                if timescale != Timescale::Tt {
                    return Err(TimeError::YearsBefore(
                        "a span before J2000.0 is on TT; give the timescale as tt",
                    ));
                }
                if !years_before_j2000.is_finite() || *years_before_j2000 < 0.0 {
                    return Err(TimeError::YearsBefore(
                        "the years before J2000.0 must be a finite number, 0 or more",
                    ));
                }
                Ok(-years_before_j2000 * JULIAN_YEAR_DAYS)
            }
        }
    }
}

/// An ISO 8601 instant in `timescale` as TT days from J2000.0.
fn instant_on_tt(instant: &str, timescale: Timescale) -> Result<f64, TimeError> {
    let bad = |why: &str| TimeError::Instant(format!("{instant:?} {why}"));
    let text = match instant.strip_suffix('Z') {
        Some(text) if timescale == Timescale::Utc => text,
        Some(_) => return Err(bad("ends in Z, which marks UTC, but its timescale is tt")),
        None => instant,
    };
    let civil = Civil::parse(text).ok_or_else(|| {
        bad("is not an ISO 8601 instant YYYY-MM-DDThh:mm:ss[.fff] with a year from 1 to 9999")
    })?;
    let day = civil.mjd();
    let seconds = civil.seconds_of_day();
    if civil.second >= 60.0 {
        let leap = timescale == Timescale::Utc
            && civil.hour == 23
            && civil.minute == 59
            && utc::ends_with_leap_second(day);
        if !leap {
            return Err(bad(
                "has a 60th second on a day that ends with no leap second",
            ));
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "an MJD of years 1 to 9999 is exact"
    )]
    let mjd = day as f64 + seconds / 86_400.0;
    match timescale {
        Timescale::Tt => Ok(mjd - J2000_MJD),
        Timescale::Utc => {
            let row = utc::row(day).ok_or_else(|| {
                bad(if day < utc::FIRST_MJD {
                    "is before 1961-01-01, when UTC began; give the time in TT"
                } else {
                    "is on or after 2027-06-28, when the leap-second table expires; give the time in TT"
                })
            })?;
            let tt_minus_utc = utc::tai_minus_utc(row, mjd) + TT_MINUS_TAI;
            Ok(mjd - J2000_MJD + tt_minus_utc / 86_400.0)
        }
    }
}

/// Whether `text` is a calendar date written `YYYY-MM-DD`, proleptic Gregorian, with a year
/// from 1 to 9999: `2026-02-31` is not one.
#[must_use]
pub fn is_calendar_date(text: &str) -> bool {
    !text.contains('T') && Civil::parse(&format!("{text}T00:00:00")).is_some()
}

/// A calendar instant, proleptic Gregorian.
struct Civil {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: f64,
}

impl Civil {
    /// `YYYY-MM-DDThh:mm:ss[.fff…]`, with a year from 1 to 9999.
    fn parse(text: &str) -> Option<Self> {
        let (date, time) = text.split_once('T')?;
        let mut date = date.split('-');
        let (year, month, day) = (date.next()?, date.next()?, date.next()?);
        let mut time = time.split(':');
        let (hour, minute, second) = (time.next()?, time.next()?, time.next()?);
        if date.next().is_some() || time.next().is_some() {
            return None;
        }
        let digits = |text: &str, width: usize| {
            (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
                .then(|| text.parse::<i64>().ok())
                .flatten()
        };
        let whole = second.split('.').next()?;
        let fraction = second.get(whole.len()..).unwrap_or_default();
        let fraction_ok = fraction.is_empty()
            || fraction.len() > 1 && fraction[1..].bytes().all(|b| b.is_ascii_digit());
        let civil = Self {
            year: digits(year, 4)?,
            month: digits(month, 2)?,
            day: digits(day, 2)?,
            hour: digits(hour, 2)?,
            minute: digits(minute, 2)?,
            second: (digits(whole, 2).is_some() && fraction_ok)
                .then(|| second.parse::<f64>().ok())
                .flatten()?,
        };
        let valid = (1..=9_999).contains(&civil.year)
            && (1..=12).contains(&civil.month)
            && (1..=days_in_month(civil.year, civil.month)).contains(&civil.day)
            && (0..24).contains(&civil.hour)
            && (0..60).contains(&civil.minute)
            && (0.0..61.0).contains(&civil.second);
        valid.then_some(civil)
    }

    /// The Modified Julian Date of the instant's day.
    fn mjd(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) + 40_587
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "hours and minutes of a day are exact"
    )]
    fn seconds_of_day(&self) -> f64 {
        (self.hour * 3_600 + self.minute * 60) as f64 + self.second
    }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a proleptic Gregorian date (Hinnant, "chrono-Compatible Low-Level
/// Date Algorithms", `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests;
