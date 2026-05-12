//! Compact human-readable report for one endpoint probe.

use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct ProbeReport {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub size_bytes: usize,
    pub top_level_keys: Vec<String>,
    pub email_hint: Option<String>,
    pub plan_hint: Option<String>,
    pub preview: Option<String>,
}

impl ProbeReport {
    pub fn format(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "URL:           {}", self.url);
        let _ = writeln!(out, "Status:        {}", self.status);
        if let Some(ct) = &self.content_type {
            let _ = writeln!(out, "Content-Type:  {ct}");
        }
        let _ = writeln!(out, "Size:          {} bytes", self.size_bytes);
        if !self.top_level_keys.is_empty() {
            let _ = writeln!(out, "Top keys:      {}", self.top_level_keys.join(", "));
        }
        if let Some(e) = &self.email_hint {
            let _ = writeln!(out, "Email hint:    {e}");
        }
        if let Some(p) = &self.plan_hint {
            let _ = writeln!(out, "Plan hint:     {p}");
        }
        if let Some(pv) = &self.preview {
            let _ = writeln!(out, "Preview:\n{pv}");
        }
        out
    }
}

/// Bytes -> ProbeReport bookkeeping. Pulls the top-level JSON keys plus
/// a small set of "is this email/plan?" hints so the operator can sanity
/// check whether the endpoint returned a useful body without echoing
/// the tokens.
pub fn distill(url: &str, status: u16, content_type: Option<String>, body: &[u8]) -> ProbeReport {
    let mut report = ProbeReport {
        url: url.to_string(),
        status,
        content_type,
        size_bytes: body.len(),
        top_level_keys: Vec::new(),
        email_hint: None,
        plan_hint: None,
        preview: None,
    };
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(obj) = value.as_object() {
            report.top_level_keys = obj.keys().cloned().collect();
            if let Some(email) = obj.get("email").and_then(|v| v.as_str()) {
                report.email_hint = Some(redact_email(email));
            }
            if let Some(plan) = obj.get("plan_name").and_then(|v| v.as_str()) {
                report.plan_hint = Some(plan.to_string());
            }
        }
    }
    if std::env::var_os("CLAUDE_WEB_PROBE_PREVIEW").is_some() {
        let preview = String::from_utf8_lossy(body)
            .chars()
            .take(500)
            .collect::<String>();
        report.preview = Some(preview);
    }
    report
}

fn redact_email(email: &str) -> String {
    if let Some((local, domain)) = email.split_once('@') {
        let visible = local.chars().take(2).collect::<String>();
        format!("{visible}***@{domain}")
    } else {
        "***".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_email_local_part() {
        assert_eq!(redact_email("jonas@skrylabs.com"), "jo***@skrylabs.com");
    }

    #[test]
    fn extracts_top_level_keys_from_object() {
        let report = distill(
            "https://x",
            200,
            None,
            br#"{"alpha":1,"beta":2,"email":"u@v.com","plan_name":"Max"}"#,
        );
        assert!(report.top_level_keys.iter().any(|k| k == "alpha"));
        assert!(report.top_level_keys.iter().any(|k| k == "beta"));
        assert_eq!(report.email_hint.as_deref(), Some("u***@v.com"));
        assert_eq!(report.plan_hint.as_deref(), Some("Max"));
    }
}
