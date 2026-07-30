use crate::models::{MetricLine, ProgressFormat};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const SESSION_PERIOD_MS: i64 = 5 * 60 * 60 * 1000;
pub const WEEK_PERIOD_MS: i64 = 7 * 24 * 60 * 60 * 1000;

const SUMMARY_BUCKETS: [(&str, &str, i64); 4] = [
    ("gemini-5h", "Session", SESSION_PERIOD_MS),
    ("gemini-weekly", "Weekly", WEEK_PERIOD_MS),
    ("3p-5h", "Claude", SESSION_PERIOD_MS),
    ("3p-weekly", "Claude Weekly", WEEK_PERIOD_MS),
];

const MODEL_BLACKLIST: [&str; 9] = [
    "MODEL_CHAT_20706",
    "MODEL_CHAT_23310",
    "MODEL_GOOGLE_GEMINI_2_5_FLASH",
    "MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING",
    "MODEL_GOOGLE_GEMINI_2_5_FLASH_LITE",
    "MODEL_GOOGLE_GEMINI_2_5_PRO",
    "MODEL_PLACEHOLDER_M19",
    "MODEL_PLACEHOLDER_M9",
    "MODEL_PLACEHOLDER_M12",
];

#[derive(Debug, Clone, PartialEq)]
pub struct AntigravityModelConfig {
    pub label: String,
    pub model_id: Option<String>,
    pub remaining_fraction: f64,
    pub reset_time_ms: Option<i64>,
}

pub struct AntigravityUsageMapper;

impl AntigravityUsageMapper {
    pub fn parse_quota_summary(data: &[u8]) -> Option<Vec<MetricLine>> {
        let root: Value = serde_json::from_slice(data).ok()?;
        let groups = root
            .get("response")
            .and_then(|response| response.get("groups"))
            .and_then(Value::as_array)
            .or_else(|| root.get("groups").and_then(Value::as_array))?;
        let known: HashSet<&str> = SUMMARY_BUCKETS.iter().map(|spec| spec.0).collect();
        let mut pooled: HashMap<&str, (f64, Option<i64>)> = HashMap::new();
        for bucket in groups
            .iter()
            .filter_map(|group| group.get("buckets").and_then(Value::as_array))
            .flatten()
        {
            let Some(id) = bucket.get("bucketId").and_then(Value::as_str) else {
                continue;
            };
            if !known.contains(id) || pooled.contains_key(id) {
                continue;
            }
            let Some(fraction) = bucket.get("remainingFraction").and_then(Value::as_f64) else {
                continue;
            };
            if !fraction.is_finite() {
                continue;
            }
            let reset_time_ms = bucket
                .get("resetTime")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_ms);
            pooled.insert(id, (fraction, reset_time_ms));
        }
        Some(
            SUMMARY_BUCKETS
                .iter()
                .filter_map(|(id, label, period)| {
                    pooled
                        .get(id)
                        .map(|(fraction, reset)| line(label, *fraction, *reset, *period))
                })
                .collect(),
        )
    }

    pub fn parse_cloud_code_models(data: &[u8]) -> Vec<AntigravityModelConfig> {
        let Ok(root) = serde_json::from_slice::<Value>(data) else {
            return vec![];
        };
        let Some(models) = root.get("models").and_then(Value::as_object) else {
            return vec![];
        };
        models
            .iter()
            .filter_map(|(key, model)| {
                if model.get("isInternal").and_then(Value::as_bool) == Some(true) {
                    return None;
                }
                let label = string_at(model, &["displayName", "label"])?;
                let model_id = string_at(model, &["model"]).or_else(|| Some(key.clone()));
                Some(config_from_quota(label, model_id, model.get("quotaInfo")))
            })
            .collect()
    }

    pub fn parse_quota_buckets(data: &[u8]) -> Vec<AntigravityModelConfig> {
        let Ok(root) = serde_json::from_slice::<Value>(data) else {
            return vec![];
        };
        root.get("buckets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|bucket| {
                let id = string_at(bucket, &["modelId"])?;
                Some(AntigravityModelConfig {
                    label: id.clone(),
                    model_id: Some(id),
                    remaining_fraction: bucket
                        .get("remainingFraction")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    reset_time_ms: bucket
                        .get("resetTime")
                        .and_then(Value::as_str)
                        .and_then(parse_rfc3339_ms),
                })
            })
            .collect()
    }

    pub fn parse_load_code_assist_plan(data: &[u8]) -> Option<String> {
        let root: Value = serde_json::from_slice(data).ok()?;
        let raw = root
            .get("paidTier")
            .and_then(|tier| tier.get("name"))
            .and_then(Value::as_str)
            .or_else(|| {
                root.get("currentTier")
                    .and_then(|tier| tier.get("name"))
                    .and_then(Value::as_str)
            });
        format_plan(raw)
    }

    pub fn parse_project(data: &[u8]) -> Option<String> {
        let root: Value = serde_json::from_slice(data).ok()?;
        root.get("cloudaicompanionProject")
            .and_then(Value::as_str)
            .and_then(trimmed)
    }

    pub fn build_lines(configs: &[AntigravityModelConfig]) -> Vec<MetricLine> {
        let blacklist: HashSet<&str> = MODEL_BLACKLIST.into_iter().collect();
        let mut pooled: HashMap<&str, (f64, Option<i64>)> = HashMap::new();
        for config in configs {
            let label = normalize_label(&config.label);
            if label.is_empty()
                || config
                    .model_id
                    .as_deref()
                    .map(|id| blacklist.contains(id))
                    .unwrap_or(false)
            {
                continue;
            }
            let pool = if label.to_lowercase().contains("gemini") {
                "Session"
            } else {
                "Claude"
            };
            match pooled.get(pool) {
                Some((fraction, _)) if config.remaining_fraction >= *fraction => {}
                _ => {
                    pooled.insert(pool, (config.remaining_fraction, config.reset_time_ms));
                }
            }
        }
        ["Session", "Claude"]
            .into_iter()
            .filter_map(|pool| {
                pooled
                    .get(pool)
                    .map(|(fraction, reset)| line(pool, *fraction, *reset, SESSION_PERIOD_MS))
            })
            .collect()
    }
}

fn config_from_quota(
    label: String,
    model_id: Option<String>,
    quota: Option<&Value>,
) -> AntigravityModelConfig {
    AntigravityModelConfig {
        label,
        model_id,
        remaining_fraction: quota
            .and_then(|value| value.get("remainingFraction"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        reset_time_ms: quota
            .and_then(|value| value.get("resetTime"))
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_ms),
    }
}

fn line(label: &str, fraction: f64, reset_time_ms: Option<i64>, period_ms: i64) -> MetricLine {
    MetricLine::Progress {
        label: label.to_string(),
        used: ((1.0 - fraction.clamp(0.0, 1.0)) * 100.0).round(),
        limit: 100.0,
        format: ProgressFormat::Percent,
        resets_at: reset_time_ms,
        period_duration_ms: Some(period_ms),
        color_hex: None,
    }
}

fn parse_rfc3339_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str).and_then(trimmed))
}

fn trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_label(label: &str) -> String {
    let label = label.trim();
    if label.ends_with(')') {
        if let Some(open) = label.rfind('(') {
            return label[..open].trim_end().to_string();
        }
    }
    label.to_string()
}

fn format_plan(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(tail) = raw.strip_prefix("Google AI ") {
        return Some(title_case(tail));
    }
    for tier in ["Ultra", "Pro", "Free"] {
        if raw.to_lowercase().contains(&tier.to_lowercase()) {
            return Some(tier.to_string());
        }
    }
    Some(title_case(raw))
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(line: &MetricLine) -> (&str, f64, Option<i64>, Option<i64>) {
        match line {
            MetricLine::Progress {
                label,
                used,
                resets_at,
                period_duration_ms,
                ..
            } => (label, *used, *resets_at, *period_duration_ms),
            _ => panic!("expected progress"),
        }
    }

    #[test]
    fn maps_bare_and_wrapped_summary_in_exact_order() {
        let groups = r#""groups":[
          {"buckets":[{"bucketId":"3p-weekly","remainingFraction":1.0},{"bucketId":"3p-5h","remainingFraction":0.4}]},
          {"buckets":[{"bucketId":"gemini-weekly","remainingFraction":0.9},{"bucketId":"gemini-5h","remainingFraction":0.75,"resetTime":"2026-07-02T16:00:00Z"}]}
        ]"#;
        let bare = AntigravityUsageMapper::parse_quota_summary(format!("{{{groups}}}").as_bytes())
            .unwrap();
        let wrapped = AntigravityUsageMapper::parse_quota_summary(
            format!("{{\"response\":{{{groups}}}}}").as_bytes(),
        )
        .unwrap();
        assert_eq!(bare, wrapped);
        let rows: Vec<_> = bare.iter().map(progress).collect();
        assert_eq!(
            rows.iter().map(|row| row.0).collect::<Vec<_>>(),
            ["Session", "Weekly", "Claude", "Claude Weekly"]
        );
        assert_eq!(
            rows.iter().map(|row| row.1).collect::<Vec<_>>(),
            [25.0, 10.0, 60.0, 0.0]
        );
        assert_eq!(rows[0].3, Some(SESSION_PERIOD_MS));
        assert_eq!(rows[1].3, Some(WEEK_PERIOD_MS));
        assert!(rows[0].2.is_some());
    }

    #[test]
    fn summary_is_lenient_exact_and_authoritative_when_empty() {
        let json = br#"{"groups":[{"buckets":["junk",{"bucketId":"gemini-image-5h","remainingFraction":0.1},{"bucketId":"3p-5h","remainingFraction":"bad"},{"bucketId":"gemini-5h","remainingFraction":-0.5},{"bucketId":"gemini-5h","remainingFraction":0.9}]}]}"#;
        let lines = AntigravityUsageMapper::parse_quota_summary(json).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(progress(&lines[0]).0, "Session");
        assert_eq!(progress(&lines[0]).1, 100.0);
        assert_eq!(
            AntigravityUsageMapper::parse_quota_summary(br#"{"groups":[]}"#),
            Some(vec![])
        );
        assert!(AntigravityUsageMapper::parse_quota_summary(b"{}").is_none());
    }

    #[test]
    fn drops_missing_fraction_without_fabricating_usage() {
        let lines = AntigravityUsageMapper::parse_quota_summary(br#"{"groups":[{"buckets":[{"bucketId":"gemini-5h"},{"bucketId":"gemini-weekly","remainingFraction":0.5}]}]}"#).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(progress(&lines[0]).0, "Weekly");
        assert_eq!(progress(&lines[0]).1, 50.0);
    }

    #[test]
    fn legacy_models_pool_worst_fraction_and_parse_plan_project() {
        let models = br#"{"models":{"g":{"model":"g","displayName":"Gemini 3 Pro (High)","quotaInfo":{"remainingFraction":0.7}},"f":{"model":"f","displayName":"Gemini Flash","quotaInfo":{"remainingFraction":0.2}},"c":{"model":"c","displayName":"Claude Opus","quotaInfo":{"remainingFraction":1}},"i":{"displayName":"Hidden","isInternal":true}}}"#;
        let lines = AntigravityUsageMapper::build_lines(
            &AntigravityUsageMapper::parse_cloud_code_models(models),
        );
        assert_eq!(progress(&lines[0]).0, "Session");
        assert_eq!(progress(&lines[0]).1, 80.0);
        assert_eq!(progress(&lines[1]).0, "Claude");
        let load = br#"{"paidTier":{"name":"Gemini Code Assist in Google One AI Pro"},"cloudaicompanionProject":" project "}"#;
        assert_eq!(
            AntigravityUsageMapper::parse_load_code_assist_plan(load).as_deref(),
            Some("Pro")
        );
        assert_eq!(
            AntigravityUsageMapper::parse_project(load).as_deref(),
            Some("project")
        );
    }
}
