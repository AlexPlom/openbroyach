use crate::http::HttpResponse;
use crate::models::{MetricKind, MetricLine, MetricValue, ProgressFormat};

pub struct ClaudeUsageMapper;

impl ClaudeUsageMapper {
    pub fn map(response: &HttpResponse) -> Result<Vec<MetricLine>, ClaudeMapperError> {
        let body = response
            .body_json()
            .ok_or(ClaudeMapperError::InvalidResponse)?;
        let mut lines = Vec::new();
        for (key, label, period) in [
            ("five_hour", "Session", 5 * 60 * 60 * 1000),
            ("seven_day", "Weekly", 7 * 24 * 60 * 60 * 1000),
            ("seven_day_sonnet", "Sonnet Weekly", 7 * 24 * 60 * 60 * 1000),
            ("seven_day_opus", "Opus Weekly", 7 * 24 * 60 * 60 * 1000),
        ] {
            if let Some(window) = body.get(key).and_then(|value| value.as_object()) {
                if let Some(utilization) = window.get("utilization").and_then(|v| v.as_f64()) {
                    lines.push(MetricLine::Progress {
                        label: label.to_string(),
                        used: utilization,
                        limit: 100.0,
                        format: ProgressFormat::Percent,
                        resets_at: window
                            .get("resets_at")
                            .and_then(|v| v.as_str())
                            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                            .map(|value| value.timestamp_millis()),
                        period_duration_ms: Some(period),
                        color_hex: None,
                    });
                }
            }
        }
        if let Some(extra) = body.get("extra_usage").and_then(|value| value.as_object()) {
            if extra.get("is_enabled").and_then(|v| v.as_bool()) == Some(true) {
                if let (Some(used), Some(limit)) = (
                    extra.get("used_credits").and_then(|v| v.as_f64()),
                    extra.get("monthly_limit").and_then(|v| v.as_f64()),
                ) {
                    lines.push(MetricLine::Values {
                        label: "Extra Usage".to_string(),
                        values: vec![
                            MetricValue::new(used / 100.0, MetricKind::Dollars),
                            MetricValue::with_label(limit / 100.0, MetricKind::Dollars, "limit"),
                        ],
                        color_hex: None,
                        expiries_at: vec![],
                        unknown_models: vec![],
                        model_breakdown: None,
                    });
                }
            }
        }
        Ok(lines)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeMapperError {
    #[error("Claude returned an invalid usage response")]
    InvalidResponse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn maps_subscription_windows_and_extra_usage() {
        let response = HttpResponse {
            status_code: 200,
            headers: HashMap::new(),
            body: br#"{"five_hour":{"utilization":12.5,"resets_at":"2026-07-31T12:00:00Z"},"seven_day":{"utilization":40},"extra_usage":{"is_enabled":true,"used_credits":125,"monthly_limit":2000}}"#.to_vec(),
        };
        let lines = ClaudeUsageMapper::map(&response).unwrap();
        assert_eq!(lines.len(), 3);
        assert!(
            matches!(&lines[0], MetricLine::Progress { label, used, .. } if label == "Session" && *used == 12.5)
        );
        assert!(matches!(&lines[2], MetricLine::Values { label, .. } if label == "Extra Usage"));
    }
}
