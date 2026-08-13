use crate::DomainError;
use serde::{Deserialize, Serialize};

/// Milliseconds since the Unix epoch. Negative timestamps are rejected.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TimestampMs(i64);

impl TimestampMs {
    /// Create a timestamp from a non-negative Unix millisecond value.
    pub fn new(value: i64) -> Result<Self, DomainError> {
        if value < 0 {
            return Err(DomainError::new(
                "timestamp_ms",
                "must not precede the Unix epoch",
            ));
        }
        Ok(Self(value))
    }

    /// Return the underlying integer representation.
    pub fn get(self) -> i64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for TimestampMs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = i64::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// A validated Gregorian calendar date without a timezone or invented time of day.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    /// Construct a real Gregorian date.
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, DomainError> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
            return Err(DomainError::new("date", "year or month is out of range"));
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(DomainError::new("date", "day is out of range"));
        }
        Ok(Self { year, month, day })
    }

    /// Parse an exact `YYYY-MM-DD` date. Ambiguous dates are represented with `TemporalValue`.
    pub fn parse_iso(value: &str) -> Result<Self, DomainError> {
        if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
            return Err(DomainError::new("date", "expected YYYY-MM-DD"));
        }
        let year = value[0..4]
            .parse::<i32>()
            .map_err(|_| DomainError::new("date", "invalid year"))?;
        let month = value[5..7]
            .parse::<u8>()
            .map_err(|_| DomainError::new("date", "invalid month"))?;
        let day = value[8..10]
            .parse::<u8>()
            .map_err(|_| DomainError::new("date", "invalid day"))?;
        Self::new(year, month, day)
    }

    /// Render the canonical exact-date form.
    pub fn to_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Add a deterministic number of calendar days.
    pub fn checked_add_days(self, days: i32) -> Result<Self, DomainError> {
        let ordinal = days_from_civil(self.year, self.month, self.day)
            .checked_add(i64::from(days))
            .ok_or_else(|| DomainError::new("date", "calendar arithmetic overflow"))?;
        let (year, month, day) = civil_from_days(ordinal);
        Self::new(year, month, day)
    }
}

/// Precision/uncertainty attached to temporal knowledge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalPrecision {
    Instant,
    Day,
    Month,
    Year,
    Before,
    After,
    Range,
    Unknown,
}

/// Temporal knowledge without false precision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemporalValue {
    ExactDate {
        date: Date,
        original: String,
    },
    Month {
        year: i32,
        month: u8,
        original: String,
    },
    Year {
        year: i32,
        original: String,
    },
    Before {
        latest: Date,
        inclusive: bool,
        original: String,
    },
    After {
        earliest: Date,
        inclusive: bool,
        original: String,
    },
    Range {
        earliest: Date,
        latest: Date,
        original: String,
    },
    Unknown {
        original: String,
    },
}

impl TemporalValue {
    /// Validate ordering and partial-date fields.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Month { year, month, .. }
                if !(1..=9999).contains(year) || !(1..=12).contains(month) =>
            {
                Err(DomainError::new("temporal", "invalid year or month"))
            }
            Self::Year { year, .. } if !(1..=9999).contains(year) => {
                Err(DomainError::new("temporal", "invalid year"))
            }
            Self::Range {
                earliest, latest, ..
            } if earliest > latest => Err(DomainError::new(
                "temporal",
                "range earliest date must not exceed latest date",
            )),
            _ => Ok(()),
        }
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

// Howard Hinnant's civil calendar algorithms, shifted to the Unix epoch.
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let adjusted_year = i64::from(year) - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (
        i32::try_from(year).unwrap_or(i32::MAX),
        u8::try_from(month).unwrap_or(12),
        u8::try_from(day).unwrap_or(31),
    )
}

#[cfg(test)]
mod tests {
    use super::{Date, TemporalValue};

    #[test]
    fn leap_year_and_month_boundary_arithmetic_is_deterministic() {
        let leap_day = Date::parse_iso("2028-02-28")
            .expect("date")
            .checked_add_days(1)
            .expect("add");
        assert_eq!(leap_day.to_iso(), "2028-02-29");
        assert_eq!(
            leap_day.checked_add_days(1).expect("add").to_iso(),
            "2028-03-01"
        );
    }

    #[test]
    fn impossible_or_ambiguous_exact_dates_are_not_accepted() {
        assert!(Date::parse_iso("2026-02-29").is_err());
        assert!(Date::parse_iso("June 2026").is_err());
    }

    #[test]
    fn reversed_uncertain_range_is_rejected() {
        let value = TemporalValue::Range {
            earliest: Date::parse_iso("2026-07-01").expect("date"),
            latest: Date::parse_iso("2026-06-01").expect("date"),
            original: "June or July".to_owned(),
        };
        assert!(value.validate().is_err());
    }
}
