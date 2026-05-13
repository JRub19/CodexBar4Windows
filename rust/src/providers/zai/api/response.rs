//! Wire types + fold for the z.ai `/api/monitor/usage/quota/limit`
//! endpoint. Ported from `ZaiUsageStats.swift`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct QuotaLimitResponse {
    pub code: i64,
    pub msg: Option<String>,
    pub success: bool,
    pub data: Option<QuotaLimitData>,
}

impl QuotaLimitResponse {
    pub fn is_success(&self) -> bool {
        self.success && self.code == 200
    }
}

#[derive(Debug, Deserialize)]
pub struct QuotaLimitData {
    #[serde(default)]
    pub limits: Vec<LimitRaw>,
    /// z.ai surfaces the plan label under several different field
    /// names; we pick the first non-empty one in priority order.
    #[serde(default, rename = "planName")]
    pub plan_name: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default, rename = "plan_type")]
    pub plan_type: Option<String>,
    #[serde(default, rename = "packageName")]
    pub package_name: Option<String>,
}

impl QuotaLimitData {
    pub fn resolved_plan_name(&self) -> Option<String> {
        for s in [
            self.plan_name.as_deref(),
            self.plan.as_deref(),
            self.plan_type.as_deref(),
            self.package_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        None
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LimitRaw {
    #[serde(rename = "type")]
    pub kind: String,
    /// Unit code: 1=days, 3=hours, 5=minutes, 6=weeks. Anything else is
    /// treated as unknown.
    pub unit: i64,
    pub number: i64,
    #[serde(default)]
    pub usage: Option<i64>,
    #[serde(default, rename = "currentValue")]
    pub current_value: Option<i64>,
    #[serde(default)]
    pub remaining: Option<i64>,
    #[serde(default)]
    pub percentage: Option<i64>,
    #[serde(default, rename = "nextResetTime")]
    pub next_reset_time_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitType {
    Tokens,
    Time,
}

impl LimitType {
    pub fn from_raw(value: &str) -> Option<Self> {
        match value {
            "TOKENS_LIMIT" => Some(LimitType::Tokens),
            "TIME_LIMIT" => Some(LimitType::Time),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitEntry {
    pub kind: LimitType,
    pub window_minutes: Option<i64>,
    pub used_percent: f64,
    pub reset_at_unix_secs: Option<i64>,
}

impl LimitRaw {
    pub fn to_entry(&self) -> Option<LimitEntry> {
        let kind = LimitType::from_raw(&self.kind)?;
        let window_minutes = match self.unit {
            1 => Some(self.number * 24 * 60), // days
            3 => Some(self.number * 60),       // hours
            5 => Some(self.number),            // minutes
            6 => Some(self.number * 7 * 24 * 60), // weeks
            _ => None,
        };
        let used_percent = computed_used_percent(self).unwrap_or_else(|| {
            self.percentage.map(|p| p as f64).unwrap_or(0.0)
        });
        let reset_at_unix_secs = self.next_reset_time_ms.map(|ms| ms / 1000);
        Some(LimitEntry {
            kind,
            window_minutes,
            used_percent: used_percent.clamp(0.0, 100.0),
            reset_at_unix_secs,
        })
    }
}

/// Derive the used% directly from `usage` (the limit), `remaining`, and
/// `currentValue`. z.ai sometimes omits these fields; in that case we
/// return None and fall back to the integer `percentage`.
fn computed_used_percent(raw: &LimitRaw) -> Option<f64> {
    let limit = raw.usage?;
    if limit <= 0 {
        return None;
    }
    let used_raw = if let Some(remaining) = raw.remaining {
        let used_from_remaining = limit - remaining;
        if let Some(current) = raw.current_value {
            used_from_remaining.max(current)
        } else {
            used_from_remaining
        }
    } else {
        raw.current_value?
    };
    let used = used_raw.clamp(0, limit);
    Some(((used as f64 / limit as f64) * 100.0).clamp(0.0, 100.0))
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ZaiFolded {
    pub primary: Option<LimitEntry>,
    pub secondary: Option<LimitEntry>,
    pub tertiary: Option<LimitEntry>,
    pub plan_name: Option<String>,
}

/// Bucket the limit list into primary/secondary/tertiary windows the
/// popup understands. Mirrors the Swift `parseUsageSnapshot` ordering:
/// - Primary  = tokens limit (the longest window when multiple)
/// - Secondary = time limit (resets at the cycle boundary)
/// - Tertiary  = shorter tokens window (session)
pub fn fold(response: &QuotaLimitResponse) -> ZaiFolded {
    let Some(data) = response.data.as_ref() else {
        return ZaiFolded::default();
    };
    let mut token_limits: Vec<LimitEntry> = Vec::new();
    let mut time_limit: Option<LimitEntry> = None;
    for raw in &data.limits {
        let Some(entry) = raw.to_entry() else {
            continue;
        };
        match entry.kind {
            LimitType::Tokens => token_limits.push(entry),
            LimitType::Time => time_limit = Some(entry),
        }
    }
    let (token_limit, session_token_limit) = if token_limits.len() >= 2 {
        token_limits.sort_by_key(|e| e.window_minutes.unwrap_or(i64::MAX));
        let shortest = token_limits.first().cloned();
        let longest = token_limits.last().cloned();
        (longest, shortest)
    } else {
        (token_limits.into_iter().next(), None)
    };
    ZaiFolded {
        primary: token_limit.clone().or_else(|| time_limit.clone()),
        secondary: if token_limit.is_some() {
            time_limit
        } else {
            None
        },
        tertiary: session_token_limit,
        plan_name: data.resolved_plan_name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_buckets_tokens_first_time_second() {
        let body = br#"{
            "code": 200, "msg": "ok", "success": true,
            "data": {
                "planName": "Coding Plan Pro",
                "limits": [
                    {"type":"TOKENS_LIMIT","unit":1,"number":1,"usage":1000,"remaining":600,"percentage":40},
                    {"type":"TIME_LIMIT","unit":1,"number":30,"usage":30,"currentValue":12,"percentage":40}
                ]
            }
        }"#;
        let resp: QuotaLimitResponse = serde_json::from_slice(body).unwrap();
        assert!(resp.is_success());
        let folded = fold(&resp);
        let primary = folded.primary.unwrap();
        assert_eq!(primary.kind, LimitType::Tokens);
        assert_eq!(primary.used_percent, 40.0);
        let secondary = folded.secondary.unwrap();
        assert_eq!(secondary.kind, LimitType::Time);
        assert_eq!(folded.plan_name.as_deref(), Some("Coding Plan Pro"));
    }

    #[test]
    fn fold_with_two_token_windows_promotes_longest_to_primary() {
        let body = br#"{
            "code": 200, "msg": "ok", "success": true,
            "data": {
                "limits": [
                    {"type":"TOKENS_LIMIT","unit":3,"number":5,"usage":100,"remaining":50,"percentage":50},
                    {"type":"TOKENS_LIMIT","unit":1,"number":7,"usage":1000,"remaining":900,"percentage":10}
                ]
            }
        }"#;
        let resp: QuotaLimitResponse = serde_json::from_slice(body).unwrap();
        let folded = fold(&resp);
        // Longest window (7 days) becomes primary, shortest (5h) becomes tertiary.
        assert_eq!(folded.primary.as_ref().unwrap().used_percent, 10.0);
        assert_eq!(folded.tertiary.as_ref().unwrap().used_percent, 50.0);
    }

    #[test]
    fn computed_percent_handles_missing_remaining_with_current_value() {
        let raw = LimitRaw {
            kind: "TOKENS_LIMIT".into(),
            unit: 1,
            number: 1,
            usage: Some(1000),
            current_value: Some(250),
            remaining: None,
            percentage: Some(99),
            next_reset_time_ms: None,
        };
        let entry = raw.to_entry().unwrap();
        // 250/1000 = 25%, overriding the raw 99 the API reported.
        assert_eq!(entry.used_percent, 25.0);
    }

    #[test]
    fn falls_back_to_api_percentage_when_quota_fields_absent() {
        let raw = LimitRaw {
            kind: "TIME_LIMIT".into(),
            unit: 1,
            number: 30,
            usage: None,
            current_value: None,
            remaining: None,
            percentage: Some(42),
            next_reset_time_ms: None,
        };
        let entry = raw.to_entry().unwrap();
        assert_eq!(entry.used_percent, 42.0);
    }

    #[test]
    fn unknown_unit_value_yields_none_window_minutes() {
        let raw = LimitRaw {
            kind: "TOKENS_LIMIT".into(),
            unit: 99,
            number: 1,
            usage: Some(100),
            current_value: None,
            remaining: Some(50),
            percentage: Some(50),
            next_reset_time_ms: None,
        };
        let entry = raw.to_entry().unwrap();
        assert!(entry.window_minutes.is_none());
    }

    #[test]
    fn unknown_kind_string_drops_entry() {
        let raw = LimitRaw {
            kind: "FUTURE_LIMIT".into(),
            unit: 1,
            number: 1,
            usage: None,
            current_value: None,
            remaining: None,
            percentage: Some(0),
            next_reset_time_ms: None,
        };
        assert!(raw.to_entry().is_none());
    }

    #[test]
    fn plan_name_falls_through_when_first_field_blank() {
        let body = br#"{
            "code": 200, "msg":"", "success": true,
            "data": {"planName": "", "plan_type": "Premium", "limits": []}
        }"#;
        let resp: QuotaLimitResponse = serde_json::from_slice(body).unwrap();
        let folded = fold(&resp);
        assert_eq!(folded.plan_name.as_deref(), Some("Premium"));
    }
}
