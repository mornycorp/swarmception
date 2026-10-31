//! The format's times: UTC, `YYYY-MM-DDTHH:MM:SSZ`, held as whole seconds since the Unix epoch.
//! The calendar arithmetic is here (no clock and no date crate): days from a civil date and back
//! (Howard Hinnant's algorithms, which hold over the whole proleptic Gregorian calendar).

use std::fmt;

/// A UTC time to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Parses `YYYY-MM-DDTHH:MM:SSZ`; anything else (another offset, fractions, a bad date) is
    /// none.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let b = text.as_bytes();
        if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
            return None;
        }
        let num = |range: std::ops::Range<usize>| -> Option<i64> { text.get(range).filter(|s| s.bytes().all(|c| c.is_ascii_digit()))?.parse().ok() };
        let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) || hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        Some(Self(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second))
    }

    /// The seconds since the epoch.
    #[must_use]
    pub const fn seconds(self) -> i64 {
        self.0
    }

    /// `(year, month, day, hour, minute, second)`.
    #[must_use]
    pub const fn civil(self) -> (i64, i64, i64, i64, i64, i64) {
        let days = self.0.div_euclid(86_400);
        let secs = self.0.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        (y, m, d, secs / 3_600, (secs % 3_600) / 60, secs % 60)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, mo, d, h, mi, s) = self.civil();
        write!(f, "{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }
}

const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a civil date.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of days since 1970-01-01.
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for text in ["1970-01-01T00:00:00Z", "2026-10-31T03:53:06Z", "2000-02-29T23:59:59Z", "1969-12-31T23:59:59Z"] {
            let t = Timestamp::parse(text).unwrap();
            assert_eq!(t.to_string(), text);
        }
        assert_eq!(Timestamp::parse("1970-01-01T00:01:00Z"), Some(Timestamp(60)));
        assert_eq!(Timestamp::parse("2026-10-31T03:53:06Z").unwrap().0, 1_793_418_786);
    }

    #[test]
    fn bad_times_are_none() {
        for text in [
            "",
            "2026-10-31 03:53:06Z",
            "2026-13-01T00:00:00Z",
            "2026-02-30T00:00:00Z",
            "2026-10-31T24:00:00Z",
            "2026-10-31T03:53:06+01",
            "+026-10-31T03:53:06Z",
            "２０26-10-31T03:53:06Z",
        ] {
            assert_eq!(Timestamp::parse(text), None, "{text}");
        }
    }
}
