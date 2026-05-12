//! Parse "Resets ..." strings from the Claude CLI `/usage` panel.
//!
//! The CLI prints reset times in several formats depending on the
//! window size and the user's locale:
//!
//! - `Resets 8pm`
//! - `Resets at 3:00pm (America/New_York)`
//! - `Resets May 14 at 11am`
//! - `Resets tomorrow at 9am`
//!
//! We extract the time-of-day and date hints into a `ResetHint`. The
//! caller folds the hint with the current local time to produce an
//! absolute unix-epoch timestamp.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetHint {
    pub hour_24: u32,
    pub minute: u32,
    pub date: ResetDate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResetDate {
    Today,
    Tomorrow,
    Absolute { month: u32, day: u32 },
}

pub fn parse(input: &str) -> Option<ResetHint> {
    let lower = input.to_ascii_lowercase();
    let lower = lower.trim();
    let lower = lower.strip_prefix("resets").unwrap_or(lower).trim();
    let lower = lower.strip_prefix("at").unwrap_or(lower).trim();

    let (date, rest) = if let Some(rest) = lower.strip_prefix("tomorrow") {
        let rest = rest.trim();
        let rest = rest.strip_prefix("at").unwrap_or(rest).trim();
        (ResetDate::Tomorrow, rest)
    } else if let Some(month_day) = parse_month_day(lower) {
        let after = strip_month_day(lower);
        let after = after.trim().strip_prefix("at").unwrap_or(after).trim();
        (
            ResetDate::Absolute {
                month: month_day.0,
                day: month_day.1,
            },
            after,
        )
    } else {
        (ResetDate::Today, lower)
    };

    let (hour, minute) = parse_clock(rest)?;
    Some(ResetHint {
        hour_24: hour,
        minute,
        date,
    })
}

/// Combine the hint with `now` to compute an absolute unix-epoch
/// second value in the local timezone.
pub fn fold_to_epoch(hint: &ResetHint, now: NaiveDateTime) -> Option<i64> {
    let today = now.date();
    let target_date = match hint.date {
        ResetDate::Today => today,
        ResetDate::Tomorrow => today.succ_opt()?,
        ResetDate::Absolute { month, day } => {
            let candidate = NaiveDate::from_ymd_opt(today.year(), month, day)?;
            if candidate < today {
                NaiveDate::from_ymd_opt(today.year() + 1, month, day)?
            } else {
                candidate
            }
        }
    };
    let mut combined = target_date.and_hms_opt(hint.hour_24, hint.minute, 0)?;
    // Today + earlier-than-now clock -> roll forward 24h.
    if hint.date == ResetDate::Today && combined < now {
        combined = combined.checked_add_signed(chrono::Duration::days(1))?;
    }
    let tz_aware = Local.from_local_datetime(&combined).single()?;
    Some(tz_aware.timestamp())
}

fn parse_clock(input: &str) -> Option<(u32, u32)> {
    // Accept "8pm", "8:30pm", "3:00pm", "8 PM", "08:30".
    let cleaned = input.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':');
    let bytes = cleaned.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut digits = String::new();
    let mut idx = 0;
    while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b':') {
        digits.push(bytes[idx] as char);
        idx += 1;
    }
    let suffix = cleaned[idx..].trim().to_ascii_lowercase();
    let (hour_str, minute_str) = match digits.split_once(':') {
        Some((h, m)) => (h, m),
        None => (digits.as_str(), "0"),
    };
    let hour: u32 = hour_str.parse().ok()?;
    let minute: u32 = minute_str.parse().ok()?;
    let hour_24 = if suffix.starts_with("am") {
        if hour == 12 {
            0
        } else {
            hour
        }
    } else if suffix.starts_with("pm") {
        if hour == 12 {
            12
        } else {
            hour + 12
        }
    } else {
        hour
    };
    if hour_24 >= 24 || minute >= 60 {
        return None;
    }
    Some((hour_24, minute))
}

fn parse_month_day(input: &str) -> Option<(u32, u32)> {
    static MONTHS: &[(&str, u32)] = &[
        ("jan", 1),
        ("feb", 2),
        ("mar", 3),
        ("apr", 4),
        ("may", 5),
        ("jun", 6),
        ("jul", 7),
        ("aug", 8),
        ("sep", 9),
        ("oct", 10),
        ("nov", 11),
        ("dec", 12),
    ];
    let trimmed = input.trim_start();
    for (name, m) in MONTHS {
        if let Some(rest) = trimmed.strip_prefix(name) {
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
            let rest = rest.trim();
            let day_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(d) = day_str.parse::<u32>() {
                return Some((*m, d));
            }
        }
    }
    None
}

fn strip_month_day(input: &str) -> &str {
    let trimmed = input.trim_start();
    // Skip month word + day number.
    let after_month = trimmed.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let after_space = after_month.trim_start();
    after_space.trim_start_matches(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn parses_resets_8pm() {
        let hint = parse("Resets 8pm").unwrap();
        assert_eq!(hint.hour_24, 20);
        assert_eq!(hint.minute, 0);
        assert_eq!(hint.date, ResetDate::Today);
    }

    #[test]
    fn parses_resets_at_300pm_with_tz() {
        let hint = parse("Resets at 3:00pm (America/New_York)").unwrap();
        assert_eq!(hint.hour_24, 15);
        assert_eq!(hint.minute, 0);
    }

    #[test]
    fn parses_resets_may_14_at_11am() {
        let hint = parse("Resets May 14 at 11am").unwrap();
        assert_eq!(hint.hour_24, 11);
        assert_eq!(hint.minute, 0);
        assert_eq!(hint.date, ResetDate::Absolute { month: 5, day: 14 });
    }

    #[test]
    fn parses_resets_tomorrow() {
        let hint = parse("Resets tomorrow at 9am").unwrap();
        assert_eq!(hint.hour_24, 9);
        assert_eq!(hint.date, ResetDate::Tomorrow);
    }

    #[test]
    fn handles_twentyfour_hour_input() {
        let hint = parse("Resets 23:45").unwrap();
        assert_eq!(hint.hour_24, 23);
        assert_eq!(hint.minute, 45);
    }

    #[test]
    fn returns_none_for_garbage() {
        assert!(parse("totally unrelated string").is_none());
    }

    #[test]
    fn folds_today_clock_into_future_epoch() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 12)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let hint = parse("Resets 8pm").unwrap();
        let epoch = fold_to_epoch(&hint, now).unwrap();
        let when = Local.timestamp_opt(epoch, 0).single().unwrap();
        assert_eq!(when.hour(), 20);
        assert_eq!(when.minute(), 0);
        assert_eq!(when.day(), 12);
    }

    #[test]
    fn folds_today_clock_in_the_past_to_tomorrow() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 12)
            .unwrap()
            .and_hms_opt(22, 0, 0)
            .unwrap();
        let hint = parse("Resets 8pm").unwrap();
        let epoch = fold_to_epoch(&hint, now).unwrap();
        let when = Local.timestamp_opt(epoch, 0).single().unwrap();
        assert_eq!(when.day(), 13);
    }
}
