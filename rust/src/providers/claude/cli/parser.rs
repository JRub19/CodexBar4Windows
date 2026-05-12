//! Parse the Claude CLI `/usage` panel into a `UsageSnapshot`.
//!
//! The panel rendering is text-mode but ANSI-coloured. We strip ANSI
//! escapes, locate the last `Settings:` header (the CLI sometimes re-
//! draws the panel above an earlier copy), then walk the table rows
//! that follow.
//!
//! Spec 40 section 4.5 lists three canonical row labels:
//! "Session", "Week", "Week (Opus)". We match on the alphanumeric-only
//! collapsed form so locale-related punctuation does not break the
//! parser.

use regex::Regex;

use crate::providers::claude::descriptor::CLAUDE_ID;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

const ANSI_PATTERN: &str = r"\x1b\[[0-9;]*[A-Za-z]";

pub fn strip_ansi(input: &str) -> String {
    let re = Regex::new(ANSI_PATTERN).expect("static regex compiles");
    re.replace_all(input, "").into_owned()
}

fn collapse_alphanum(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn label_to_key(collapsed: &str) -> Option<(&'static str, &'static str)> {
    // (canonical key, friendly label).
    match collapsed {
        "session" | "sessionwindow" | "5h" | "5hr" => Some(("five_hour", "Session")),
        "week" | "weekly" | "weekwindow" | "7d" | "7day" => Some(("seven_day", "Week")),
        "weekopus" | "opusweek" | "weeklyopus" | "weekopuswindow" => {
            Some(("seven_day_opus", "Week (Opus)"))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedRow {
    pub key: String,
    pub label: String,
    pub percent_used: f64,
    pub reset_hint: Option<String>,
}

pub fn parse_panel(raw: &str) -> Vec<ParsedRow> {
    let stripped = strip_ansi(raw);
    // Use the last "Settings:" as the anchor when the CLI redraws.
    let body = match stripped.rfind("Settings:") {
        Some(idx) => &stripped[idx..],
        None => stripped.as_str(),
    };
    let mut out = Vec::new();
    for line in body.lines() {
        let mut parts = line.splitn(2, ':');
        let label_raw = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let rest = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let collapsed = collapse_alphanum(label_raw);
        let Some((key, label)) = label_to_key(collapsed.as_str()) else {
            continue;
        };
        let percent = match parse_percent(rest) {
            Some(p) => p,
            None => continue,
        };
        let reset_hint = extract_reset_hint(rest);
        out.push(ParsedRow {
            key: key.to_string(),
            label: label.to_string(),
            percent_used: percent,
            reset_hint,
        });
    }
    out
}

fn parse_percent(rest: &str) -> Option<f64> {
    let re = Regex::new(r"(\d{1,3}(?:\.\d+)?)%").ok()?;
    let cap = re.captures(rest)?;
    cap.get(1)?.as_str().parse().ok()
}

fn extract_reset_hint(rest: &str) -> Option<String> {
    let idx = rest.to_ascii_lowercase().find("resets")?;
    Some(rest[idx..].trim().to_string())
}

/// Fold the parsed rows into a `UsageSnapshot` keyed by a synthetic
/// account token (the CLI does not expose the email in `/usage`).
pub fn snapshot_from_rows(
    rows: &[ParsedRow],
    account_token: impl Into<String>,
    captured_at_unix_secs: i64,
) -> UsageSnapshot {
    let windows = rows
        .iter()
        .map(|r| NamedRateWindow {
            key: r.key.clone(),
            window: RateWindow {
                label: r.label.clone(),
                used: r.percent_used,
                allotted: Some(100.0),
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        })
        .collect();
    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(CLAUDE_ID, account_token),
        windows,
        credits: None,
        cost: None,
        account_display_name: None,
        account_email: None,
        plan_name: None,
        captured_at_unix_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
\x1b[1mSettings:\x1b[0m\n\
Session : 25% used. Resets 8pm\n\
Week    : 60% used. Resets May 14 at 11am\n\
Week (Opus) : 10% used. Resets May 14 at 11am\n\
";

    #[test]
    fn strips_ansi_escapes() {
        let stripped = strip_ansi("\x1b[31mhi\x1b[0m there");
        assert_eq!(stripped, "hi there");
    }

    #[test]
    fn parses_three_canonical_rows() {
        let rows = parse_panel(FIXTURE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key, "five_hour");
        assert_eq!(rows[0].percent_used, 25.0);
        assert_eq!(rows[1].key, "seven_day");
        assert_eq!(rows[2].key, "seven_day_opus");
    }

    #[test]
    fn captures_reset_hint_substring() {
        let rows = parse_panel(FIXTURE);
        assert_eq!(rows[0].reset_hint.as_deref(), Some("Resets 8pm"));
    }

    #[test]
    fn skips_unrecognized_labels() {
        let rows = parse_panel("Settings:\nProject : 50% used\nSession : 10% used. Resets 8pm");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "five_hour");
    }

    #[test]
    fn anchors_on_last_settings_header() {
        let raw = format!("Settings:\nold row : 99%\n{FIXTURE}");
        let rows = parse_panel(&raw);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].percent_used, 25.0);
    }

    #[test]
    fn snapshot_fold_keeps_row_order() {
        let rows = parse_panel(FIXTURE);
        let snap = snapshot_from_rows(&rows, "claude:cli-1", 0);
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.windows[0].key, "five_hour");
        assert_eq!(snap.identity.account_token, "claude:cli-1");
    }
}
