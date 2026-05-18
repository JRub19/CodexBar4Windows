//! Reusable API/session-token provider support for Windows-only providers.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use crate::core::ProviderId;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::credits::{CreditUnit, CreditsSnapshot};
use crate::providers::models::provider_cost::{ProviderCostSnapshot, ServiceCost};
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;
use crate::secrets::token_account::TokenKind;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
pub struct CommonProviderSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub env_vars: &'static [&'static str],
    pub auth_hint: AuthHint,
    pub endpoint: EndpointSpec,
}

#[derive(Clone, Copy)]
pub enum AuthHint {
    Bearer,
    Cookie,
    RawHeader(&'static str),
    None,
}

#[derive(Clone, Copy)]
pub enum EndpointSpec {
    JsonGet(&'static str),
    JsonPost {
        url: &'static str,
        body: &'static str,
    },
    OpenAiAdmin,
    OpenAiCredits,
    Codebuff,
    AugmentCli,
}

pub struct CommonApiStrategy {
    spec: CommonProviderSpec,
    client: Client,
}

impl CommonApiStrategy {
    pub fn new(spec: CommonProviderSpec) -> Result<Self, ProviderFetchError> {
        let client = Client::builder()
            .connect_timeout(TIMEOUT)
            .user_agent(concat!("codexbar4windows/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(Self { spec, client })
    }
}

#[async_trait]
impl Strategy for CommonApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        match self.spec.endpoint {
            EndpointSpec::AugmentCli => FetchStrategy::CLI,
            _ => FetchStrategy::ApiKey,
        }
    }

    async fn fetch(&self, ctx: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        match self.spec.endpoint {
            EndpointSpec::AugmentCli => fetch_augment_cli(self.spec).await,
            EndpointSpec::OpenAiAdmin => self.fetch_openai_admin(ctx).await,
            EndpointSpec::OpenAiCredits => self.fetch_openai_credits(ctx).await,
            EndpointSpec::Codebuff => self.fetch_codebuff(ctx).await,
            EndpointSpec::JsonGet(url) => self.fetch_json(ctx, "GET", url, None).await,
            EndpointSpec::JsonPost { url, body } => {
                self.fetch_json(ctx, "POST", url, Some(body)).await
            }
        }
    }
}

impl CommonApiStrategy {
    async fn resolve_secret(
        &self,
        ctx: &ProviderFetchContext,
    ) -> Result<String, ProviderFetchError> {
        for env in self.spec.env_vars {
            if let Ok(value) = std::env::var(env) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }
        let provider_id = self.spec.id;
        let store = ctx.runtime.tokens.clone();
        tokio::task::spawn_blocking(move || store.active_for(provider_id))
            .await
            .map_err(|e| ProviderFetchError::Network(format!("token store join failed: {e}")))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map(|account| account.value)
            .filter(|value| !value.trim().is_empty())
            .ok_or(ProviderFetchError::NoToken(self.spec.id))
    }

    async fn fetch_json(
        &self,
        ctx: &ProviderFetchContext,
        method: &str,
        url: &str,
        body: Option<&str>,
    ) -> Result<UsageSnapshot, ProviderFetchError> {
        let secret = self.resolve_secret(ctx).await?;
        let mut request = match method {
            "POST" => self.client.post(url),
            _ => self.client.get(url),
        }
        .timeout(TIMEOUT)
        .header("Accept", "application/json");
        request = apply_auth(request, self.spec.auth_hint, &secret);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let response = request.send().await.map_err(map_reqwest)?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        if status == 401 || status == 403 {
            return Err(ProviderFetchError::Unauthorized);
        }
        if !(200..=299).contains(&status) {
            return Err(ProviderFetchError::Network(format!(
                "{} returned HTTP {status}",
                self.spec.display_name
            )));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        Ok(snapshot_from_value(self.spec, &value, &secret))
    }

    async fn fetch_openai_admin(
        &self,
        ctx: &ProviderFetchContext,
    ) -> Result<UsageSnapshot, ProviderFetchError> {
        let secret = self.resolve_secret(ctx).await?;
        let now = unix_now();
        let start = now.saturating_sub(30 * 24 * 60 * 60);
        let costs_url = format!(
            "https://api.openai.com/v1/organization/costs?start_time={start}&bucket_width=1d&group_by=line_item"
        );
        let usage_url = format!(
            "https://api.openai.com/v1/organization/usage/completions?start_time={start}&bucket_width=1d&group_by=model"
        );
        let costs = self.openai_get(&costs_url, &secret).await?;
        let usage = self
            .openai_get(&usage_url, &secret)
            .await
            .unwrap_or(Value::Null);
        Ok(openai_snapshot(self.spec, &costs, &usage, &secret))
    }

    async fn fetch_openai_credits(
        &self,
        ctx: &ProviderFetchContext,
    ) -> Result<UsageSnapshot, ProviderFetchError> {
        self.fetch_json(
            ctx,
            "GET",
            "https://api.openai.com/v1/dashboard/billing/credit_grants",
            None,
        )
        .await
    }

    async fn fetch_codebuff(
        &self,
        ctx: &ProviderFetchContext,
    ) -> Result<UsageSnapshot, ProviderFetchError> {
        if let Some(token) = read_codebuff_cli_token() {
            return self
                .fetch_json_with_secret(
                    "POST",
                    "https://www.codebuff.com/api/v1/usage",
                    Some(r#"{"fingerprintId":"codexbar-usage"}"#),
                    &token,
                )
                .await;
        }
        self.fetch_json(
            ctx,
            "POST",
            "https://www.codebuff.com/api/v1/usage",
            Some(r#"{"fingerprintId":"codexbar-usage"}"#),
        )
        .await
    }

    async fn fetch_json_with_secret(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        secret: &str,
    ) -> Result<UsageSnapshot, ProviderFetchError> {
        let mut request = match method {
            "POST" => self.client.post(url),
            _ => self.client.get(url),
        }
        .timeout(TIMEOUT)
        .header("Accept", "application/json");
        request = apply_auth(request, self.spec.auth_hint, secret);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let response = request.send().await.map_err(map_reqwest)?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        if status == 401 || status == 403 {
            return Err(ProviderFetchError::Unauthorized);
        }
        if !(200..=299).contains(&status) {
            return Err(ProviderFetchError::Network(format!(
                "{} returned HTTP {status}",
                self.spec.display_name
            )));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        Ok(snapshot_from_value(self.spec, &value, secret))
    }

    async fn openai_get(&self, url: &str, secret: &str) -> Result<Value, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .timeout(TIMEOUT)
            .header("Accept", "application/json")
            .bearer_auth(secret)
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        if status == 401 || status == 403 {
            return Err(ProviderFetchError::Unauthorized);
        }
        if !(200..=299).contains(&status) {
            return Err(ProviderFetchError::Network(format!(
                "OpenAI Admin API returned HTTP {status}"
            )));
        }
        serde_json::from_slice(&bytes).map_err(|e| ProviderFetchError::ParseError(e.to_string()))
    }
}

fn apply_auth(
    request: reqwest::RequestBuilder,
    hint: AuthHint,
    secret: &str,
) -> reqwest::RequestBuilder {
    match hint {
        AuthHint::Bearer => request.bearer_auth(secret),
        AuthHint::Cookie => request.header("Cookie", secret),
        AuthHint::RawHeader(header) => request.header(header, secret),
        AuthHint::None => request,
    }
}

fn map_reqwest(error: reqwest::Error) -> ProviderFetchError {
    if error.is_timeout() {
        ProviderFetchError::Timeout {
            budget_ms: TIMEOUT.as_millis() as u64,
        }
    } else {
        ProviderFetchError::Network(error.to_string())
    }
}

fn snapshot_from_value(spec: CommonProviderSpec, value: &Value, secret: &str) -> UsageSnapshot {
    let balance = first_number(
        value,
        &[
            "balance",
            "credits",
            "credit",
            "remaining",
            "available",
            "availableCredits",
            "available_credits",
            "total_available",
            "grant_amount",
        ],
    )
    .unwrap_or(0.0);
    let used = first_number(
        value,
        &["used", "usage", "total_usage", "spent", "consumed"],
    )
    .unwrap_or(0.0);
    let total = first_number(value, &["total", "limit", "quota", "hard_limit_usd"]).or({
        if balance > 0.0 || used > 0.0 {
            Some(balance + used)
        } else {
            None
        }
    });
    let used_percent = total
        .filter(|total| *total > 0.0)
        .map(|total| (used / total * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let window = NamedRateWindow {
        key: "credits".into(),
        window: RateWindow {
            label: "Credits".into(),
            used: used_percent,
            allotted: Some(100.0),
            reset_at_unix_secs: first_i64(value, &["reset_at", "resetAt", "cycle_end"]),
            pace_delta_percent: None,
        },
    };
    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(
            ProviderId(spec.id),
            format!("{}:{}", spec.id, short_secret(secret)),
        ),
        windows: vec![window],
        credits: Some(CreditsSnapshot {
            balance,
            unit: CreditUnit::Credits,
            recent_events: Vec::new(),
        }),
        cost: None,
        account_display_name: first_string(value, &["name", "username", "email"]),
        account_email: first_email(value),
        plan_name: first_string(value, &["plan", "plan_name", "tier", "subscription"]),
        captured_at_unix_secs: unix_now(),
    }
}

fn openai_snapshot(
    spec: CommonProviderSpec,
    costs: &Value,
    usage: &Value,
    secret: &str,
) -> UsageSnapshot {
    let total = sum_numbers_by_key(costs, &["amount", "amount_usd", "cost", "cost_usd"]);
    let requests = sum_numbers_by_key(usage, &["num_model_requests", "requests"]);
    let tokens = sum_numbers_by_key(usage, &["input_tokens", "output_tokens", "num_tokens"]);
    let services = collect_service_costs(costs);
    let window_value = if requests > 0.0 { requests } else { tokens };
    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(
            ProviderId(spec.id),
            format!("{}:{}", spec.id, short_secret(secret)),
        ),
        windows: vec![NamedRateWindow {
            key: "requests".into(),
            window: RateWindow {
                label: if requests > 0.0 { "Requests" } else { "Tokens" }.into(),
                used: window_value,
                allotted: None,
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        }],
        credits: None,
        cost: Some(ProviderCostSnapshot {
            current_cycle_usd: total,
            previous_cycle_usd: None,
            last_30_days_usd: vec![total],
            daily: Vec::new(),
            total_window_usd: total,
            updated_at_unix_secs: unix_now(),
            breakdown_by_service: services,
        }),
        account_display_name: Some("OpenAI Admin API".into()),
        account_email: None,
        plan_name: Some(format!("30d spend ${total:.2}")),
        captured_at_unix_secs: unix_now(),
    }
}

async fn fetch_augment_cli(spec: CommonProviderSpec) -> Result<UsageSnapshot, ProviderFetchError> {
    let output = Command::new("auggie")
        .args(["account", "status"])
        .output()
        .map_err(|e| ProviderFetchError::PluginUnavailable(format!("auggie: {e}")))?;
    if !output.status.success() {
        return Err(ProviderFetchError::PluginUnavailable(
            "auggie account status failed".into(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let remaining = first_number_in_text(&text, &["remaining", "credits"]).unwrap_or(0.0);
    let used = first_number_in_text(&text, &["used"]).unwrap_or(0.0);
    let total = if remaining + used > 0.0 {
        remaining + used
    } else {
        remaining
    };
    Ok(UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(ProviderId(spec.id), "augment:cli"),
        windows: vec![NamedRateWindow {
            key: "credits".into(),
            window: RateWindow {
                label: "Credits".into(),
                used: if total > 0.0 {
                    used / total * 100.0
                } else {
                    0.0
                },
                allotted: Some(100.0),
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        }],
        credits: Some(CreditsSnapshot {
            balance: remaining,
            unit: CreditUnit::Credits,
            recent_events: Vec::new(),
        }),
        cost: None,
        account_display_name: Some("Augment CLI".into()),
        account_email: None,
        plan_name: None,
        captured_at_unix_secs: unix_now(),
    })
}

fn read_codebuff_cli_token() -> Option<String> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let path = home
        .join(".config")
        .join("manicode")
        .join("credentials.json");
    let bytes = std::fs::read(path).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value
        .pointer("/default/authToken")
        .and_then(Value::as_str)
        .or_else(|| value.get("authToken").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn first_number(value: &Value, keys: &[&str]) -> Option<f64> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(n) = map.get(*key).and_then(value_as_f64) {
                    return Some(n);
                }
            }
            map.values().find_map(|v| first_number(v, keys))
        }
        Value::Array(values) => values.iter().find_map(|v| first_number(v, keys)),
        _ => None,
    }
}

fn first_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    first_number(value, keys).map(|n| n as i64)
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(s) = map.get(*key).and_then(Value::as_str) {
                    return Some(s.to_string());
                }
            }
            map.values().find_map(|v| first_string(v, keys))
        }
        Value::Array(values) => values.iter().find_map(|v| first_string(v, keys)),
        _ => None,
    }
}

fn first_email(value: &Value) -> Option<String> {
    first_string(value, &["email"]).filter(|s| s.contains('@'))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
        .or_else(|| value.as_str()?.parse::<f64>().ok())
}

fn sum_numbers_by_key(value: &Value, keys: &[&str]) -> f64 {
    match value {
        Value::Object(map) => {
            let here = keys
                .iter()
                .filter_map(|key| map.get(*key).and_then(value_as_f64))
                .sum::<f64>();
            here + map
                .values()
                .map(|v| sum_numbers_by_key(v, keys))
                .sum::<f64>()
        }
        Value::Array(values) => values.iter().map(|v| sum_numbers_by_key(v, keys)).sum(),
        _ => 0.0,
    }
}

fn collect_service_costs(value: &Value) -> Vec<ServiceCost> {
    let mut out = Vec::new();
    collect_service_costs_inner(value, &mut out);
    out
}

fn collect_service_costs_inner(value: &Value, out: &mut Vec<ServiceCost>) {
    match value {
        Value::Object(map) => {
            let name = map
                .get("line_item")
                .or_else(|| map.get("service"))
                .or_else(|| map.get("model"))
                .and_then(Value::as_str);
            let cost = map
                .get("amount")
                .or_else(|| map.get("amount_usd"))
                .or_else(|| map.get("cost"))
                .and_then(value_as_f64);
            if let (Some(name), Some(usd)) = (name, cost) {
                out.push(ServiceCost {
                    service_name: name.to_string(),
                    current_cycle_usd: usd,
                });
            }
            for child in map.values() {
                collect_service_costs_inner(child, out);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_service_costs_inner(child, out);
            }
        }
        _ => {}
    }
}

fn first_number_in_text(text: &str, labels: &[&str]) -> Option<f64> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if labels.iter().any(|label| lower.contains(label)) {
            for token in line.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
                if let Ok(n) = token.parse::<f64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn short_secret(secret: &str) -> String {
    secret
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

pub fn token_kind_for_auth(auth: AuthHint) -> TokenKind {
    match auth {
        AuthHint::Cookie => TokenKind::Cookie,
        AuthHint::Bearer | AuthHint::RawHeader(_) | AuthHint::None => TokenKind::ApiKey,
    }
}

pub fn strategy(spec: CommonProviderSpec) -> Vec<Arc<dyn Strategy>> {
    match CommonApiStrategy::new(spec) {
        Ok(strategy) => vec![Arc::new(strategy) as Arc<dyn Strategy>],
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_snapshot_extracts_balance_and_usage() {
        let value: Value = serde_json::json!({
            "data": { "available_credits": 42.0, "used": 8.0, "total": 50.0 },
            "email": "user@example.com"
        });
        let snap = snapshot_from_value(
            CommonProviderSpec {
                id: "test",
                display_name: "Test",
                env_vars: &[],
                auth_hint: AuthHint::Bearer,
                endpoint: EndpointSpec::JsonGet("https://example.com"),
            },
            &value,
            "sk-test",
        );
        assert_eq!(snap.credits.unwrap().balance, 42.0);
        assert_eq!(snap.windows[0].window.used, 16.0);
        assert_eq!(snap.account_email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn openai_snapshot_sums_nested_costs_and_usage() {
        let costs: Value = serde_json::json!({
            "data": [{ "results": [
                { "line_item": "gpt-4.1", "amount": 1.25 },
                { "line_item": "gpt-5", "amount": 2.75 }
            ]}]
        });
        let usage: Value = serde_json::json!({
            "data": [{ "results": [{ "num_model_requests": 10 }]}]
        });
        let snap = openai_snapshot(
            CommonProviderSpec {
                id: "openai",
                display_name: "OpenAI API",
                env_vars: &[],
                auth_hint: AuthHint::Bearer,
                endpoint: EndpointSpec::OpenAiAdmin,
            },
            &costs,
            &usage,
            "sk-admin",
        );
        assert_eq!(snap.cost.unwrap().total_window_usd, 4.0);
        assert_eq!(snap.windows[0].window.used, 10.0);
    }
}
