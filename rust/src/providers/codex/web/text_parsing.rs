//! Locale-aware number + reset-time parser for the OpenAI dashboard
//! scrape. Spec 41 §5.6 lists every format:
//!
//! Numbers
//! - US:   `1,234.56`
//! - EU:   `1.234,56`
//! - FR:   `1 234,56` with U+202F or U+00A0 thin spaces
//! - Plain `1234`, `1234.5`, `1234,5`
//!
//! Heuristic: if the scraped text contains "crédit" (French) the parser
//! treats `,` as decimal and `.` as thousand separator; otherwise it
//! uses the disambiguation rule "the rightmost separator that has 1-2
//! digits after it is the decimal."

use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberLocale {
    /// `,` thousand, `.` decimal. The default for English.
    Us,
    /// `.` thousand, `,` decimal. Used for FR/DE/many EU locales.
    Eu,
}

/// Infer the locale from a snippet of context text. We look for the
/// French keyword "crédit" (with accent) which is the most reliable
/// marker that the scrape ran on a French-language dashboard.
pub fn locale_from_context(text: &str) -> NumberLocale {
    let lower = text.to_lowercase();
    if lower.contains("crédit") {
        NumberLocale::Eu
    } else {
        NumberLocale::Us
    }
}

/// Parse a localized number string into f64. Returns `None` on
/// malformed input. Handles thin spaces (`U+202F`, `U+00A0`) as
/// thousand separators in addition to the locale-specific punctuation.
pub fn parse_number(raw: &str, locale: NumberLocale) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202F}' && *c != '\u{00A0}')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let normalized = match locale {
        NumberLocale::Us => cleaned.replace(',', ""),
        NumberLocale::Eu => {
            // Strip `.` thousand separator; promote `,` to `.` as decimal.
            cleaned.replace('.', "").replace(',', ".")
        }
    };
    normalized.parse::<f64>().ok()
}

/// Heuristic number parser when the caller is unsure of locale. Falls
/// back to the "rightmost 1-2 digit group is decimal" rule.
pub fn parse_number_heuristic(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202F}' && *c != '\u{00A0}')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let last_dot = cleaned.rfind('.');
    let last_comma = cleaned.rfind(',');
    match (last_dot, last_comma) {
        (Some(d), Some(c)) => {
            if d > c {
                parse_number(&cleaned, NumberLocale::Us)
            } else {
                parse_number(&cleaned, NumberLocale::Eu)
            }
        }
        (Some(d), None) => {
            // A single `.` after a 1-3 digit tail is decimal; otherwise
            // it is a thousands separator.
            let tail_len = cleaned.len() - d - 1;
            if (1..=3).contains(&tail_len) && tail_len != 3 {
                parse_number(&cleaned, NumberLocale::Us)
            } else if tail_len == 3 {
                // Ambiguous; assume thousands separator.
                parse_number(&cleaned, NumberLocale::Eu)
            } else {
                parse_number(&cleaned, NumberLocale::Us)
            }
        }
        (None, Some(c)) => {
            let tail_len = cleaned.len() - c - 1;
            if (1..=2).contains(&tail_len) {
                parse_number(&cleaned, NumberLocale::Eu)
            } else {
                parse_number(&cleaned, NumberLocale::Us)
            }
        }
        (None, None) => cleaned.parse::<f64>().ok(),
    }
}

/// Parse a reset-time hint from scraped DOM text. The dashboard prints
/// a variety of phrasings: "Resets tomorrow at 9:00 AM", "Resets in 3
/// hours", "Resets May 14 at 11:00 AM". We return a typed `ResetHint`
/// so the caller can fold it against the current local time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResetHint {
    InMinutes(u32),
    InHours(u32),
    InDays(u32),
    AbsoluteDate { month: u32, day: u32, hour_24: u32 },
    Tomorrow { hour_24: u32, minute: u32 },
}

pub fn parse_reset_hint(text: &str) -> Option<ResetHint> {
    let lower = text.trim().to_ascii_lowercase();
    let lower = lower.strip_prefix("resets").unwrap_or(&lower).trim();

    // "in 3 minutes" / "in 3h" / "in 2 days"
    if let Some(rest) = lower.strip_prefix("in") {
        let rest = rest.trim();
        if let Some((n, unit)) = parse_count_unit(rest) {
            return match unit.as_str() {
                "minute" | "minutes" | "min" | "m" => Some(ResetHint::InMinutes(n)),
                "hour" | "hours" | "hr" | "h" => Some(ResetHint::InHours(n)),
                "day" | "days" | "d" => Some(ResetHint::InDays(n)),
                _ => None,
            };
        }
    }

    if let Some(rest) = lower.strip_prefix("tomorrow") {
        let rest = rest.trim().strip_prefix("at").unwrap_or(rest).trim();
        if let Some((h, m)) = parse_clock(rest) {
            return Some(ResetHint::Tomorrow {
                hour_24: h,
                minute: m,
            });
        }
    }

    if let Some(prefix) = month_prefix(lower) {
        let after = lower.split_at(prefix.matched_len).1.trim_start();
        let day_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(day) = day_str.parse::<u32>() {
            let rest = after
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim();
            let rest = rest.strip_prefix("at").unwrap_or(rest).trim();
            if let Some((h, _m)) = parse_clock(rest) {
                return Some(ResetHint::AbsoluteDate {
                    month: prefix.month,
                    day,
                    hour_24: h,
                });
            }
        }
    }

    None
}

struct MonthPrefix {
    month: u32,
    matched_len: usize,
}

fn month_prefix(input: &str) -> Option<MonthPrefix> {
    const MONTHS: &[(&str, u32)] = &[
        ("january", 1),
        ("february", 2),
        ("march", 3),
        ("april", 4),
        ("may", 5),
        ("june", 6),
        ("july", 7),
        ("august", 8),
        ("september", 9),
        ("october", 10),
        ("november", 11),
        ("december", 12),
        ("jan", 1),
        ("feb", 2),
        ("mar", 3),
        ("apr", 4),
        ("jun", 6),
        ("jul", 7),
        ("aug", 8),
        ("sep", 9),
        ("oct", 10),
        ("nov", 11),
        ("dec", 12),
    ];
    for (name, month) in MONTHS {
        if input.starts_with(name) {
            return Some(MonthPrefix {
                month: *month,
                matched_len: name.len(),
            });
        }
    }
    None
}

fn parse_count_unit(input: &str) -> Option<(u32, String)> {
    let regex = Regex::new(r"^(\d+)\s*([a-zA-Z]+)").ok()?;
    let cap = regex.captures(input)?;
    let count: u32 = cap.get(1)?.as_str().parse().ok()?;
    let unit = cap.get(2)?.as_str().to_ascii_lowercase();
    Some((count, unit))
}

fn parse_clock(input: &str) -> Option<(u32, u32)> {
    let regex = Regex::new(r"^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?").ok()?;
    let cap = regex.captures(input)?;
    let hour: u32 = cap.get(1)?.as_str().parse().ok()?;
    let minute: u32 = cap
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let suffix = cap.get(3).map(|m| m.as_str().to_ascii_lowercase());
    let hour_24 = match suffix.as_deref() {
        Some("pm") => {
            if hour == 12 {
                12
            } else {
                hour + 12
            }
        }
        Some("am") => {
            if hour == 12 {
                0
            } else {
                hour
            }
        }
        _ => hour,
    };
    if hour_24 >= 24 || minute >= 60 {
        return None;
    }
    Some((hour_24, minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_us_format() {
        assert_eq!(parse_number("1,234.56", NumberLocale::Us), Some(1234.56));
        assert_eq!(parse_number("42", NumberLocale::Us), Some(42.0));
        assert_eq!(parse_number("0.5", NumberLocale::Us), Some(0.5));
    }

    #[test]
    fn parses_eu_format() {
        assert_eq!(parse_number("1.234,56", NumberLocale::Eu), Some(1234.56));
        assert_eq!(parse_number("0,5", NumberLocale::Eu), Some(0.5));
    }

    #[test]
    fn handles_thin_space_thousand_separators() {
        // U+202F NARROW NO-BREAK SPACE
        let raw = "1\u{202F}234,56";
        assert_eq!(parse_number(raw, NumberLocale::Eu), Some(1234.56));
        // U+00A0 NO-BREAK SPACE
        let raw = "1\u{00A0}234.56";
        assert_eq!(parse_number(raw, NumberLocale::Us), Some(1234.56));
    }

    #[test]
    fn heuristic_picks_decimal_separator_from_rightmost_short_group() {
        // 1.234 with 3 digits after `.` is ambiguous → treat as thousand.
        assert_eq!(parse_number_heuristic("1.234"), Some(1234.0));
        // 1.5 with 1 digit after `.` is decimal.
        assert_eq!(parse_number_heuristic("1.5"), Some(1.5));
        // 1,5 with 1 digit after `,` is decimal (EU).
        assert_eq!(parse_number_heuristic("1,5"), Some(1.5));
        // Both separators present → rightmost is decimal.
        assert_eq!(parse_number_heuristic("1.234,56"), Some(1234.56));
        assert_eq!(parse_number_heuristic("1,234.56"), Some(1234.56));
    }

    #[test]
    fn french_context_triggers_eu_locale() {
        let locale = locale_from_context("Vous avez utilisé 0,5 crédit aujourd'hui");
        assert_eq!(locale, NumberLocale::Eu);
        assert_eq!(parse_number("0,5", locale), Some(0.5));
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(parse_number("", NumberLocale::Us).is_none());
        assert!(parse_number_heuristic("   ").is_none());
    }

    #[test]
    fn parses_reset_in_minutes() {
        assert_eq!(
            parse_reset_hint("Resets in 30 minutes"),
            Some(ResetHint::InMinutes(30))
        );
        assert_eq!(
            parse_reset_hint("Resets in 5m"),
            Some(ResetHint::InMinutes(5))
        );
    }

    #[test]
    fn parses_reset_in_hours_and_days() {
        assert_eq!(
            parse_reset_hint("Resets in 3 hours"),
            Some(ResetHint::InHours(3))
        );
        assert_eq!(
            parse_reset_hint("Resets in 2 days"),
            Some(ResetHint::InDays(2))
        );
    }

    #[test]
    fn parses_reset_tomorrow_at_clock() {
        assert_eq!(
            parse_reset_hint("Resets tomorrow at 9:00 AM"),
            Some(ResetHint::Tomorrow {
                hour_24: 9,
                minute: 0
            })
        );
        assert_eq!(
            parse_reset_hint("Resets tomorrow at 3:30 PM"),
            Some(ResetHint::Tomorrow {
                hour_24: 15,
                minute: 30
            })
        );
    }

    #[test]
    fn parses_reset_absolute_date() {
        assert_eq!(
            parse_reset_hint("Resets May 14 at 11:00 AM"),
            Some(ResetHint::AbsoluteDate {
                month: 5,
                day: 14,
                hour_24: 11
            })
        );
        assert_eq!(
            parse_reset_hint("Resets Jan 1 at 12:00 AM"),
            Some(ResetHint::AbsoluteDate {
                month: 1,
                day: 1,
                hour_24: 0
            })
        );
    }

    #[test]
    fn unparseable_text_returns_none() {
        assert!(parse_reset_hint("hello").is_none());
        assert!(parse_reset_hint("Resets when pigs fly").is_none());
    }
}
