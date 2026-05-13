//! Wire types and fold for the Gemini Cloud Code quota response.
//! Ported from `parseAPIResponse` and `toUsageSnapshot` in
//! `GeminiStatusProbe.swift`.

use chrono::DateTime;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct QuotaResponse {
    #[serde(default)]
    pub buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaBucket {
    #[serde(default, rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(default, rename = "resetTime")]
    pub reset_time: Option<String>,
    #[serde(default, rename = "modelId")]
    pub model_id: Option<String>,
    #[serde(default, rename = "tokenType")]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuota {
    pub model_id: String,
    /// Percent of the daily window still available (0..=100).
    pub percent_left: f64,
    pub reset_at_unix_secs: Option<i64>,
}

/// Bucket models into the three Gemini tiers the popup renders, keeping
/// the lowest quota per model. Mirrors the macOS classifier verbatim.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GeminiTierQuotas {
    pub pro: Option<ModelQuota>,
    pub flash: Option<ModelQuota>,
    pub flash_lite: Option<ModelQuota>,
}

pub fn fold_buckets(response: &QuotaResponse) -> Vec<ModelQuota> {
    use std::collections::BTreeMap;

    let Some(buckets) = response.buckets.as_ref() else {
        return Vec::new();
    };
    // Group by modelId, keep the lowest fraction (the most pessimistic
    // signal — usually input-token bucket).
    let mut map: BTreeMap<String, (f64, Option<String>)> = BTreeMap::new();
    for bucket in buckets {
        let (Some(model), Some(fraction)) = (bucket.model_id.as_ref(), bucket.remaining_fraction)
        else {
            continue;
        };
        let entry = map
            .entry(model.clone())
            .or_insert((fraction, bucket.reset_time.clone()));
        if fraction < entry.0 {
            *entry = (fraction, bucket.reset_time.clone());
        }
    }
    map.into_iter()
        .map(|(model_id, (fraction, reset))| ModelQuota {
            model_id,
            percent_left: (fraction * 100.0).clamp(0.0, 100.0),
            reset_at_unix_secs: reset.as_deref().and_then(parse_reset_unix_secs),
        })
        .collect()
}

pub fn classify_models(quotas: &[ModelQuota]) -> GeminiTierQuotas {
    let mut pro_min: Option<&ModelQuota> = None;
    let mut flash_min: Option<&ModelQuota> = None;
    let mut flash_lite_min: Option<&ModelQuota> = None;

    for q in quotas {
        let lower = q.model_id.to_ascii_lowercase();
        if is_flash_lite(&lower) {
            if flash_lite_min.is_none_or(|m| q.percent_left < m.percent_left) {
                flash_lite_min = Some(q);
            }
        } else if is_flash(&lower) {
            if flash_min.is_none_or(|m| q.percent_left < m.percent_left) {
                flash_min = Some(q);
            }
        } else if is_pro(&lower) && pro_min.is_none_or(|m| q.percent_left < m.percent_left) {
            pro_min = Some(q);
        }
    }

    GeminiTierQuotas {
        pro: pro_min.cloned(),
        flash: flash_min.cloned(),
        flash_lite: flash_lite_min.cloned(),
    }
}

fn is_flash_lite(model_id: &str) -> bool {
    model_id.contains("flash-lite")
}

fn is_flash(model_id: &str) -> bool {
    model_id.contains("flash") && !is_flash_lite(model_id)
}

fn is_pro(model_id: &str) -> bool {
    model_id.contains("pro")
}

fn parse_reset_unix_secs(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_buckets_keeping_lowest_per_model() {
        let body = br#"{"buckets": [
            {"modelId": "gemini-2.5-pro", "remainingFraction": 0.8, "tokenType": "output"},
            {"modelId": "gemini-2.5-pro", "remainingFraction": 0.2, "tokenType": "input"},
            {"modelId": "gemini-2.5-flash", "remainingFraction": 0.5},
            {"modelId": "gemini-2.5-flash-lite", "remainingFraction": 0.9}
        ]}"#;
        let response: QuotaResponse = serde_json::from_slice(body).unwrap();
        let quotas = fold_buckets(&response);
        let pro = quotas
            .iter()
            .find(|q| q.model_id == "gemini-2.5-pro")
            .unwrap();
        // Lowest fraction wins → 0.2 → 20%
        assert_eq!(pro.percent_left, 20.0);
        let flash = quotas
            .iter()
            .find(|q| q.model_id == "gemini-2.5-flash")
            .unwrap();
        assert_eq!(flash.percent_left, 50.0);
    }

    #[test]
    fn classify_groups_into_pro_flash_and_flash_lite() {
        let quotas = vec![
            ModelQuota {
                model_id: "gemini-2.5-pro".into(),
                percent_left: 20.0,
                reset_at_unix_secs: None,
            },
            ModelQuota {
                model_id: "gemini-2.5-flash".into(),
                percent_left: 50.0,
                reset_at_unix_secs: None,
            },
            ModelQuota {
                model_id: "gemini-2.5-flash-lite".into(),
                percent_left: 90.0,
                reset_at_unix_secs: None,
            },
        ];
        let tiers = classify_models(&quotas);
        assert_eq!(tiers.pro.unwrap().percent_left, 20.0);
        assert_eq!(tiers.flash.unwrap().percent_left, 50.0);
        assert_eq!(tiers.flash_lite.unwrap().percent_left, 90.0);
    }

    #[test]
    fn flash_lite_does_not_double_count_in_flash_bucket() {
        let quotas = vec![ModelQuota {
            model_id: "gemini-2.5-flash-lite-preview".into(),
            percent_left: 75.0,
            reset_at_unix_secs: None,
        }];
        let tiers = classify_models(&quotas);
        assert!(tiers.flash.is_none());
        assert_eq!(tiers.flash_lite.unwrap().percent_left, 75.0);
    }

    #[test]
    fn classify_picks_minimum_when_multiple_models_in_tier() {
        let quotas = vec![
            ModelQuota {
                model_id: "gemini-2.5-flash".into(),
                percent_left: 50.0,
                reset_at_unix_secs: None,
            },
            ModelQuota {
                model_id: "gemini-2.0-flash".into(),
                percent_left: 10.0,
                reset_at_unix_secs: None,
            },
        ];
        let tiers = classify_models(&quotas);
        assert_eq!(tiers.flash.unwrap().percent_left, 10.0);
    }

    #[test]
    fn parses_reset_time_to_unix_secs() {
        let body = br#"{"buckets": [
            {"modelId": "gemini-2.5-pro", "remainingFraction": 0.5, "resetTime": "2026-06-01T00:00:00Z"}
        ]}"#;
        let response: QuotaResponse = serde_json::from_slice(body).unwrap();
        let quotas = fold_buckets(&response);
        assert_eq!(quotas[0].reset_at_unix_secs, Some(1_780_272_000));
    }

    #[test]
    fn buckets_with_missing_model_or_fraction_are_skipped() {
        let body = br#"{"buckets": [
            {"remainingFraction": 0.5},
            {"modelId": "gemini-2.5-pro"},
            {"modelId": "gemini-2.5-pro", "remainingFraction": 0.3}
        ]}"#;
        let response: QuotaResponse = serde_json::from_slice(body).unwrap();
        let quotas = fold_buckets(&response);
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].percent_left, 30.0);
    }
}
