//! Watch the CLI's stdout for known prompt strings and emit the right
//! keystroke so the strategy can run unattended. Spec 40 section 4.4
//! names the prompts; we keep the substring list narrow so we never
//! type into a prompt we don't recognize.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    /// Send `y` followed by CR (the CLI is in raw mode).
    AffirmTrust,
    /// Send the Cursor Position Report response. The CLI sometimes
    /// queries for the cursor position when starting up; failing to
    /// answer wedges it.
    CursorPositionReport,
}

impl Response {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Response::AffirmTrust => b"y\r",
            Response::CursorPositionReport => b"\x1b[1;1R",
        }
    }
}

/// Inspect a chunk of CLI output and return any auto responses to emit.
/// The caller passes a rolling tail buffer so prompts split across
/// chunks still match.
pub fn responses_for(tail: &str) -> Vec<Response> {
    let normalized = normalize(tail);
    let mut out = Vec::new();
    if normalized.contains("doyoutrustthefilesinthisfolder")
        || normalized.contains("trustthecreatorsofthesefiles")
    {
        out.push(Response::AffirmTrust);
    }
    // CPR query: ESC [ 6 n
    if tail.contains("\x1b[6n") {
        out.push(Response::CursorPositionReport);
    }
    out
}

fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_trust_prompt() {
        let raw = "Do you trust the files in this folder?";
        let responses = responses_for(raw);
        assert_eq!(responses, vec![Response::AffirmTrust]);
        assert_eq!(Response::AffirmTrust.bytes(), b"y\r");
    }

    #[test]
    fn detects_cpr_query() {
        let raw = "loading\x1b[6n";
        let responses = responses_for(raw);
        assert_eq!(responses, vec![Response::CursorPositionReport]);
        assert_eq!(Response::CursorPositionReport.bytes(), b"\x1b[1;1R");
    }

    #[test]
    fn ignores_unrelated_output() {
        let responses = responses_for("Welcome to Claude. Type a prompt and press enter.");
        assert!(responses.is_empty());
    }

    #[test]
    fn handles_prompt_with_decorative_unicode() {
        let raw = "🤖  Do you trust the files in this folder?";
        let responses = responses_for(raw);
        assert_eq!(responses, vec![Response::AffirmTrust]);
    }
}
