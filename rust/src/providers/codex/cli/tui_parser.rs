//! Pure-text parser for the Codex CLI `/status` panel. Ported from
//! `CodexStatusProbe.parse` in the macOS Swift source.
//!
//! Real `codex` does not speak JSON-RPC; the interactive TUI emits a
//! few lines containing the user's plan info when the user types
//! `/status`. We strip ANSI, find the `Credits:`, `5h limit`, and
//! `Weekly limit` lines, and extract the percentages + reset hints.

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CodexTuiSnapshot {
    /// Credits balance (free-tier accounts), if present.
    pub credits: Option<f64>,
    /// 0..=100, percentage REMAINING (matches the `X% left` token in the
    /// TUI; subtract from 100 to render a "used" bar).
    pub five_hour_percent_left: Option<i64>,
    pub weekly_percent_left: Option<i64>,
    /// Raw reset hint as printed by the TUI, e.g. `13:42 on 5 Jun`.
    /// The caller can re-anchor it against `Local::now()` if needed.
    pub five_hour_reset_hint: Option<String>,
    pub weekly_reset_hint: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty TUI output (codex never printed anything within budget)")]
    Empty,
    #[error("codex reported `data not available yet`")]
    DataNotAvailable,
    #[error("codex update prompt is blocking /status; run `bun install -g @openai/codex`")]
    UpdateRequired,
    #[error("could not find credits, 5h, or weekly limits in TUI output")]
    NoUsageData,
}

/// Strip CSI ANSI escape sequences. Matches the Swift regex literal
/// `\[[0-?]*[ -/]*[@-~]`.
pub fn strip_ansi(text: &str) -> String {
    // We compile lazily on the first call rather than at module init
    // so the regex crate's static-cell doesn't show up in startup
    // profiles.
    use once_cell::sync::Lazy;
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").expect("ansi regex"));
    RE.replace_all(text, "").into_owned()
}

pub fn parse(text: &str) -> Result<CodexTuiSnapshot, ParseError> {
    let cleaned = strip_ansi(text);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("data not available yet") {
        return Err(ParseError::DataNotAvailable);
    }
    if contains_update_prompt(&lower) {
        return Err(ParseError::UpdateRequired);
    }

    let credits = first_number(r"(?i)Credits:\s*([0-9][0-9.,]*)", &cleaned);
    let five_line = first_line_matching(r"(?i)5h limit[^\n]*", &cleaned);
    let week_line = first_line_matching(r"(?i)Weekly limit[^\n]*", &cleaned);
    let five_pct = five_line.as_deref().and_then(percent_left_from_line);
    let week_pct = week_line.as_deref().and_then(percent_left_from_line);
    let five_reset = five_line.as_deref().and_then(reset_string_from_line);
    let week_reset = week_line.as_deref().and_then(reset_string_from_line);

    if credits.is_none() && five_pct.is_none() && week_pct.is_none() {
        return Err(ParseError::NoUsageData);
    }

    Ok(CodexTuiSnapshot {
        credits,
        five_hour_percent_left: five_pct,
        weekly_percent_left: week_pct,
        five_hour_reset_hint: five_reset,
        weekly_reset_hint: week_reset,
    })
}

fn contains_update_prompt(lower: &str) -> bool {
    lower.contains("update available") && lower.contains("codex")
}

pub fn first_number(pattern: &str, text: &str) -> Option<f64> {
    let re = Regex::new(pattern).ok()?;
    let caps = re.captures(text)?;
    let raw = caps.get(1)?.as_str();
    parse_number(raw)
}

pub fn first_line_matching(pattern: &str, text: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    let m = re.find(text)?;
    Some(m.as_str().to_string())
}

pub fn percent_left_from_line(line: &str) -> Option<i64> {
    let re = Regex::new(r"(?i)([0-9]{1,3})%\s+left").ok()?;
    let caps = re.captures(line)?;
    caps.get(1)?.as_str().parse::<i64>().ok()
}

pub fn reset_string_from_line(line: &str) -> Option<String> {
    let re = Regex::new(r"(?i)resets?\s+(.+)").ok()?;
    let caps = re.captures(line)?;
    Some(caps.get(1)?.as_str().trim().to_string())
}

/// Parse a number string with locale-flexible separators. Mirrors the
/// macOS `parseNumber`: handle `1,234.56`, `1.234,56`, NBSP / narrow
/// no-break space, and the special case where a `,` is purely a
/// decimal separator (e.g. `1,5`).
pub fn parse_number(raw: &str) -> Option<f64> {
    let mut text = raw.trim().to_string();
    text = text.replace(['\u{00A0}', '\u{202F}', ' '], "");
    let has_comma = text.contains(',');
    let has_dot = text.contains('.');
    if has_comma && has_dot {
        let last_comma = text.rfind(',').unwrap_or(0);
        let last_dot = text.rfind('.').unwrap_or(0);
        if last_comma > last_dot {
            text = text.replace('.', "");
            text = text.replace(',', ".");
        } else {
            text = text.replace(',', "");
        }
    } else if has_comma {
        let thousand_re = Regex::new(r"^\d{1,3}(,\d{3})+$").ok()?;
        if thousand_re.is_match(&text) {
            text = text.replace(',', "");
        } else {
            text = text.replace(',', ".");
        }
    } else if has_dot {
        let thousand_re = Regex::new(r"^\d{1,3}(\.\d{3})+$").ok()?;
        if thousand_re.is_match(&text) {
            text = text.replace('.', "");
        }
    }
    text.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STATUS_PANEL: &str = "\x1b[33m  Plan        Pro\x1b[0m\n\
        \x1b[1mCredits:\x1b[0m 12.50\n\
        \x1b[36m5h limit\x1b[0m  78% left  resets 13:42 on 5 Jun\n\
        \x1b[36mWeekly limit\x1b[0m  42% left  resets 09:00 on Sun 8 Jun\n";

    #[test]
    fn strips_ansi_csi_sequences() {
        let cleaned = strip_ansi("\x1b[1mhello\x1b[0m world");
        assert_eq!(cleaned, "hello world");
    }

    #[test]
    fn parses_credits_five_hour_and_weekly() {
        let snap = parse(SAMPLE_STATUS_PANEL).unwrap();
        assert_eq!(snap.credits, Some(12.5));
        assert_eq!(snap.five_hour_percent_left, Some(78));
        assert_eq!(snap.weekly_percent_left, Some(42));
        assert_eq!(snap.five_hour_reset_hint.as_deref(), Some("13:42 on 5 Jun"));
        assert_eq!(
            snap.weekly_reset_hint.as_deref(),
            Some("09:00 on Sun 8 Jun")
        );
    }

    #[test]
    fn empty_input_maps_to_empty_error() {
        assert_eq!(parse("   "), Err(ParseError::Empty));
    }

    #[test]
    fn data_not_available_yet_surfaces_dedicated_error() {
        let err = parse("Data not available yet, please retry").unwrap_err();
        assert_eq!(err, ParseError::DataNotAvailable);
    }

    #[test]
    fn update_prompt_surfaces_update_required_error() {
        let err = parse(
            "Update available: codex 1.2.3 → 1.3.0\nRun `bun install -g @openai/codex` to continue.",
        )
        .unwrap_err();
        assert_eq!(err, ParseError::UpdateRequired);
    }

    #[test]
    fn no_usage_data_when_panel_has_no_relevant_fields() {
        let err = parse("Welcome to codex! Use /help for commands.").unwrap_err();
        assert_eq!(err, ParseError::NoUsageData);
    }

    #[test]
    fn parses_credits_with_thousands_separator() {
        let snap = parse("Credits: 1,234.56\n5h limit  50% left  resets 09:00").unwrap();
        assert_eq!(snap.credits, Some(1234.56));
    }

    #[test]
    fn parses_credits_with_eu_decimal_separator() {
        let snap = parse("Credits: 12,5\n5h limit  10% left  resets 09:00").unwrap();
        assert_eq!(snap.credits, Some(12.5));
    }

    #[test]
    fn parses_credits_with_dot_thousands_separator() {
        let snap = parse("Credits: 1.234\n5h limit  10% left  resets 09:00").unwrap();
        // `1.234` is ambiguous on its own; macOS source treats it as 1234
        // because the pattern `^\d{1,3}(\.\d{3})+$` matches. Mirror that.
        assert_eq!(snap.credits, Some(1234.0));
    }

    #[test]
    fn percent_left_from_line_ignores_other_percentages() {
        assert_eq!(
            percent_left_from_line("Weekly limit  73% left  resets 13:42 (40% of pool)"),
            Some(73)
        );
    }

    #[test]
    fn reset_string_strips_resets_prefix() {
        assert_eq!(
            reset_string_from_line("5h limit  10% left  resets 13:42 on 5 Jun"),
            Some("13:42 on 5 Jun".to_string())
        );
    }

    #[test]
    fn five_h_limit_only_still_yields_snapshot() {
        let snap = parse("5h limit  88% left  resets 09:00").unwrap();
        assert_eq!(snap.five_hour_percent_left, Some(88));
        assert!(snap.weekly_percent_left.is_none());
        assert!(snap.credits.is_none());
    }
}
