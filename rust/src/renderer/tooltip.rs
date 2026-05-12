//! Tooltip builder for the Windows tray icon.
//!
//! Windows truncates tray tooltips at 128 characters. Multi line strings
//! use CRLF, not LF. We construct human friendly text from a structured
//! `TooltipInputs` and clamp gracefully when an unusually verbose
//! provider name plus account label would overflow.

const MAX_TOOLTIP_LEN: usize = 127; // 128 minus the null terminator.

#[derive(Clone, Debug, Default)]
pub struct TooltipInputs {
    /// First line, typically the product name. Required.
    pub title: String,
    /// Subsequent lines, one per provider summary. The builder joins
    /// them with CRLF and truncates the whole tooltip at 128 chars.
    pub lines: Vec<String>,
}

impl TooltipInputs {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn with_line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    /// Build the final tooltip string. Always CRLF separated; total
    /// length capped at 127 chars (the Windows tray limit). When the
    /// content overruns, the last line is truncated with a single
    /// ellipsis character.
    pub fn build(&self) -> String {
        let mut out = self.title.clone();
        for line in &self.lines {
            if out.len() + 2 + line.len() > MAX_TOOLTIP_LEN {
                // Truncate this line so total stays under the limit.
                let space_left = MAX_TOOLTIP_LEN.saturating_sub(out.len()).saturating_sub(3);
                if space_left > 0 {
                    out.push_str("\r\n");
                    let mut truncated: String = line.chars().take(space_left).collect();
                    truncated.push('\u{2026}');
                    out.push_str(&truncated);
                }
                break;
            }
            out.push_str("\r\n");
            out.push_str(line);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tooltip_is_just_the_title() {
        let t = TooltipInputs::new("CodexBar4Windows").build();
        assert_eq!(t, "CodexBar4Windows");
    }

    #[test]
    fn multi_line_uses_crlf() {
        let t = TooltipInputs::new("CodexBar")
            .with_line("Claude 67% session")
            .with_line("41% week")
            .build();
        assert_eq!(t, "CodexBar\r\nClaude 67% session\r\n41% week");
    }

    #[test]
    fn long_tooltip_is_truncated_with_ellipsis() {
        let long_line = "x".repeat(200);
        let t = TooltipInputs::new("CodexBar").with_line(long_line).build();
        assert!(
            t.chars().count() <= MAX_TOOLTIP_LEN,
            "tooltip exceeded limit: {} chars",
            t.chars().count()
        );
        assert!(t.ends_with('\u{2026}'));
    }

    #[test]
    fn truncation_drops_lines_that_do_not_fit() {
        let t = TooltipInputs::new("CodexBar")
            .with_line("a".repeat(60))
            .with_line("b".repeat(60))
            .with_line("c".repeat(60))
            .build();
        assert!(t.chars().count() <= MAX_TOOLTIP_LEN);
    }
}
