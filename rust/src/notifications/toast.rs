//! Notification copy + the typed `NotificationToast` the Tauri shell
//! hands to `tauri-plugin-notification`. Strings ported from the macOS
//! `SessionQuotaNotifier` notification copy.

use super::thresholds::ThresholdEvent;
use super::transition::SessionTransition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationToast {
    /// Stable id prefix used to dedup repeated toasts in Tauri's
    /// notification queue. Same value the macOS source uses so cross-
    /// platform reasoning matches.
    pub id: String,
    pub title: String,
    pub body: String,
    /// Whether the OS sound should play. Threshold warnings on macOS
    /// play `Glass`; session transitions are silent. The Tauri side
    /// honours this when constructing the notification.
    pub sound: bool,
}

pub fn copy_for_transition(
    provider_id: &str,
    provider_display: &str,
    transition: SessionTransition,
) -> Option<NotificationToast> {
    let (title, body) = match transition {
        SessionTransition::Depleted => (
            format!("{provider_display} session depleted"),
            "0% left. Will notify when it's available again.".to_string(),
        ),
        SessionTransition::Restored => (
            format!("{provider_display} session restored"),
            "Session quota is available again.".to_string(),
        ),
        SessionTransition::None => return None,
    };
    let kind = match transition {
        SessionTransition::Depleted => "depleted",
        SessionTransition::Restored => "restored",
        SessionTransition::None => return None,
    };
    Some(NotificationToast {
        id: format!("session-{provider_id}-{kind}"),
        title,
        body,
        sound: false,
    })
}

pub fn copy_for_threshold(
    provider_id: &str,
    provider_display: &str,
    window_label: &str,
    window_key: &str,
    event: &ThresholdEvent,
    current_remaining: f64,
) -> NotificationToast {
    let pct = format_percent(current_remaining);
    let title = format!("{provider_display} {window_label} quota low");
    let body = format!(
        "{pct} left. Reached your {}% {window_label} warning threshold.",
        event.threshold,
    );
    NotificationToast {
        id: format!(
            "quota-warning-{provider_id}-{window_key}-{}",
            event.threshold
        ),
        title,
        body,
        sound: true,
    }
}

fn format_percent(value: f64) -> String {
    let rounded = value.clamp(0.0, 100.0).round() as i64;
    format!("{rounded}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depleted_copy_matches_macos_strings() {
        let toast = copy_for_transition("claude", "Claude", SessionTransition::Depleted).unwrap();
        assert_eq!(toast.title, "Claude session depleted");
        assert_eq!(
            toast.body,
            "0% left. Will notify when it's available again."
        );
        assert_eq!(toast.id, "session-claude-depleted");
        assert!(!toast.sound, "transitions are silent on macOS");
    }

    #[test]
    fn restored_copy_matches_macos_strings() {
        let toast = copy_for_transition("codex", "Codex", SessionTransition::Restored).unwrap();
        assert_eq!(toast.title, "Codex session restored");
        assert_eq!(toast.id, "session-codex-restored");
    }

    #[test]
    fn none_transition_yields_no_toast() {
        assert!(copy_for_transition("claude", "Claude", SessionTransition::None).is_none());
    }

    #[test]
    fn threshold_copy_includes_remaining_and_threshold() {
        let event = ThresholdEvent { threshold: 25 };
        let toast = copy_for_threshold("claude", "Claude", "Session", "session", &event, 22.4);
        assert_eq!(toast.title, "Claude Session quota low");
        assert_eq!(
            toast.body,
            "22% left. Reached your 25% Session warning threshold."
        );
        assert_eq!(toast.id, "quota-warning-claude-session-25");
        assert!(toast.sound, "threshold warnings play OS sound");
    }

    #[test]
    fn percent_rounding_clamps_to_0_100() {
        let event = ThresholdEvent { threshold: 10 };
        let above = copy_for_threshold("x", "X", "Session", "session", &event, 105.7);
        assert!(above.body.starts_with("100%"));
        let below = copy_for_threshold("x", "X", "Session", "session", &event, -3.2);
        assert!(below.body.starts_with("0%"));
    }
}
