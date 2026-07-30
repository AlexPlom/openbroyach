use crate::http::HttpResponse;
use crate::models::*;

pub const SESSION_PERIOD_MS: i64 = 5 * 3600 * 1000; // 5 hours
pub const WEEKLY_PERIOD_MS: i64 = 7 * 24 * 3600 * 1000; // 1 week
const CREDIT_USD_RATE: f64 = 0.04;

#[derive(Debug, Clone)]
pub struct CodexMappedUsage {
    pub plan: Option<String>,
    pub lines: Vec<MetricLine>,
}

pub struct CodexUsageMapper;

#[derive(Clone, Copy, PartialEq)]
enum WindowKind {
    Session,
    Weekly,
}

impl CodexUsageMapper {
    pub fn map_usage_response(
        response: &HttpResponse,
        reset_credits: Option<&HttpResponse>,
        now: i64,
    ) -> Result<CodexMappedUsage, CodexMapperError> {
        let body = response
            .body_json()
            .ok_or(CodexMapperError::InvalidResponse)?;

        let mut lines: Vec<MetricLine> = Vec::new();

        // Rate limit windows (session / weekly)
        let rate_limit = body.get("rate_limit").and_then(|v| v.as_object());
        if let Some(rl) = rate_limit {
            if let Some(line) = Self::window_line(
                "Session",
                Self::classified_window(rl, WindowKind::Session),
                SESSION_PERIOD_MS,
                now,
            ) {
                lines.push(line);
            }
            if let Some(line) = Self::window_line(
                "Weekly",
                Self::classified_window(rl, WindowKind::Weekly),
                WEEKLY_PERIOD_MS,
                now,
            ) {
                lines.push(line);
            }
        }

        // Spark model-specific limits
        if let Some(additional) = body
            .get("additional_rate_limits")
            .and_then(|v| v.as_array())
        {
            for entry in additional {
                if let Some(obj) = entry.as_object() {
                    let is_spark = obj
                        .get("limit_name")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.get("metered_feature").and_then(|v| v.as_str()))
                        .map(|s| s.to_lowercase().contains("spark"))
                        .unwrap_or(false);
                    if is_spark {
                        if let Some(rl) = obj.get("rate_limit").and_then(|v| v.as_object()) {
                            if let Some(line) = Self::window_line(
                                "Spark",
                                Self::classified_window(rl, WindowKind::Session),
                                SESSION_PERIOD_MS,
                                now,
                            ) {
                                lines.push(line);
                            }
                            if let Some(line) = Self::window_line(
                                "Spark Weekly",
                                Self::classified_window(rl, WindowKind::Weekly),
                                WEEKLY_PERIOD_MS,
                                now,
                            ) {
                                lines.push(line);
                            }
                        }
                    }
                }
            }
        }

        // Rate limit reset credits
        if let Some(resets) = Self::read_reset_credits(&body, reset_credits) {
            lines.push(MetricLine::Values {
                label: "Rate Limit Resets".to_string(),
                values: vec![MetricValue::with_label(
                    resets.0 as f64,
                    MetricKind::Count,
                    "available",
                )],
                color_hex: None,
                expiries_at: resets.1,
                unknown_models: vec![],
                model_breakdown: None,
            });
        }

        // Credits (remaining flex credits)
        if let Some(remaining) = Self::read_credits_remaining(response, &body) {
            let credits = (remaining.floor() as i64).max(0);
            let usd = credits as f64 * CREDIT_USD_RATE;
            lines.push(MetricLine::Values {
                label: "Credits".to_string(),
                values: vec![
                    MetricValue::new(usd, MetricKind::Dollars),
                    MetricValue::with_label(credits as f64, MetricKind::Count, "credits"),
                ],
                color_hex: None,
                expiries_at: vec![],
                unknown_models: vec![],
                model_breakdown: None,
            });
        }

        let plan = body
            .get("plan_type")
            .and_then(|v| v.as_str())
            .map(Self::format_codex_plan);

        Ok(CodexMappedUsage { plan, lines })
    }

    fn window_line(
        label: &str,
        window: Option<&serde_json::Map<String, serde_json::Value>>,
        default_period_ms: i64,
        now: i64,
    ) -> Option<MetricLine> {
        let used_percent = window.and_then(|w| w.get("used_percent").and_then(|v| v.as_f64()))?;
        let period_ms = window
            .and_then(|w| {
                w.get("limit_window_seconds")
                    .and_then(|v| v.as_f64())
                    .map(|s| (s * 1000.0) as i64)
            })
            .unwrap_or(default_period_ms);
        let resets_at = Self::reset_date(window, now);

        Some(MetricLine::Progress {
            label: label.to_string(),
            used: used_percent,
            limit: 100.0,
            format: ProgressFormat::Percent,
            resets_at,
            period_duration_ms: Some(period_ms),
            color_hex: None,
        })
    }

    fn classified_window<'a>(
        rate_limit: &'a serde_json::Map<String, serde_json::Value>,
        kind: WindowKind,
    ) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
        let primary = rate_limit
            .get("primary_window")
            .and_then(|value| value.as_object());
        let secondary = rate_limit
            .get("secondary_window")
            .and_then(|value| value.as_object());
        let candidates = [primary, secondary];

        candidates
            .iter()
            .flatten()
            .copied()
            .find(|window| Self::window_kind(window) == Some(kind))
            .or_else(|| {
                let fallback = match kind {
                    WindowKind::Session => primary,
                    WindowKind::Weekly => secondary,
                };
                fallback.filter(|window| Self::window_kind(window).is_none())
            })
    }

    fn window_kind(window: &serde_json::Map<String, serde_json::Value>) -> Option<WindowKind> {
        let period_ms = window
            .get("limit_window_seconds")
            .and_then(|value| value.as_f64())
            .map(|seconds| (seconds * 1000.0) as i64)?;
        match period_ms {
            SESSION_PERIOD_MS => Some(WindowKind::Session),
            WEEKLY_PERIOD_MS => Some(WindowKind::Weekly),
            _ => None,
        }
    }

    fn reset_date(
        window: Option<&serde_json::Map<String, serde_json::Value>>,
        now: i64,
    ) -> Option<i64> {
        let window = window?;
        if let Some(reset_at) = window.get("reset_at").and_then(|v| v.as_f64()) {
            return Some(Self::epoch_millis(reset_at));
        }
        if let Some(reset_after) = window.get("reset_after_seconds").and_then(|v| v.as_f64()) {
            return Some(now + (reset_after * 1000.0) as i64);
        }
        None
    }

    fn read_reset_credits(
        body: &serde_json::Value,
        reset_credits: Option<&HttpResponse>,
    ) -> Option<(i64, Vec<i64>)> {
        let source = reset_credits
            .and_then(|rc| {
                if (200..300).contains(&rc.status_code) {
                    rc.body_json()
                        .filter(|j| j.get("available_count").and_then(|v| v.as_f64()).is_some())
                } else {
                    None
                }
            })
            .or_else(|| {
                body.get("rate_limit_reset_credits")
                    .and_then(|v| v.as_object().map(|o| o.clone().into()))
            })?;

        let count = source.get("available_count")?.as_f64()?;
        if count < 0.0 {
            return None;
        }

        let expiries: Vec<i64> = source
            .get("credits")
            .and_then(|v| v.as_array())
            .map(|credits| {
                credits
                    .iter()
                    .filter(|c| {
                        c.get("status")
                            .and_then(|s| s.as_str())
                            .map_or(true, |s| s == "available")
                    })
                    .filter_map(|c| {
                        c.get("expires_at").and_then(|e| {
                            e.as_str()
                                .and_then(|s| {
                                    chrono::DateTime::parse_from_rfc3339(s)
                                        .ok()
                                        .map(|dt| dt.timestamp_millis())
                                })
                                .or_else(|| e.as_f64().map(Self::epoch_millis))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(((count.floor() as i64).max(0), expiries))
    }

    fn read_credits_remaining(response: &HttpResponse, body: &serde_json::Value) -> Option<f64> {
        if let Some(credits) = body.get("credits").and_then(|v| v.as_object()) {
            if let Some(balance) = credits.get("balance").and_then(|v| v.as_f64()) {
                return Some(balance);
            }
            if credits.get("has_credits").and_then(|v| v.as_bool()) == Some(false) {
                return Some(0.0);
            }
        }
        response
            .header("x-codex-credits-balance")
            .and_then(|v| v.parse::<f64>().ok())
    }

    fn format_codex_plan(raw: &str) -> String {
        match raw.to_lowercase().as_str() {
            "prolite" => "Pro 5x".to_string(),
            "pro" => "Pro 20x".to_string(),
            _ => raw.to_string(),
        }
    }

    fn epoch_millis(timestamp: f64) -> i64 {
        if timestamp.abs() < 100_000_000_000.0 {
            (timestamp * 1000.0) as i64
        } else {
            timestamp as i64
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodexMapperError {
    #[error("Invalid response")]
    InvalidResponse,
    #[error("Token expired")]
    TokenExpired,
    #[error("Request failed: {0}")]
    RequestFailed(u16),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn normalizes_epoch_second_reset_and_expiry_timestamps() {
        let response = HttpResponse {
            status_code: 200,
            headers: HashMap::new(),
            body: serde_json::to_vec(&serde_json::json!({
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 42.0,
                        "reset_at": 1_700_003_600
                    }
                },
                "rate_limit_reset_credits": {
                    "available_count": 1,
                    "credits": [{
                        "status": "available",
                        "expires_at": 1_700_086_400
                    }]
                }
            }))
            .unwrap(),
        };

        let mapped =
            CodexUsageMapper::map_usage_response(&response, None, 1_700_000_000_000).unwrap();

        assert!(matches!(
            mapped.lines.first(),
            Some(MetricLine::Progress {
                resets_at: Some(1_700_003_600_000),
                ..
            })
        ));
        assert!(matches!(
            mapped.lines.get(1),
            Some(MetricLine::Values { expiries_at, .. })
                if expiries_at == &[1_700_086_400_000]
        ));
    }

    #[test]
    fn classifies_rate_limit_windows_by_duration_instead_of_slot() {
        let response = HttpResponse {
            status_code: 200,
            headers: HashMap::new(),
            body: serde_json::to_vec(&serde_json::json!({
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 70.0,
                        "limit_window_seconds": 604_800
                    },
                    "secondary_window": {
                        "used_percent": 20.0,
                        "limit_window_seconds": 18_000
                    }
                }
            }))
            .unwrap(),
        };

        let mapped = CodexUsageMapper::map_usage_response(&response, None, 0).unwrap();

        assert!(matches!(
            mapped.lines.first(),
            Some(MetricLine::Progress { label, used: 20.0, .. }) if label == "Session"
        ));
        assert!(matches!(
            mapped.lines.get(1),
            Some(MetricLine::Progress { label, used: 70.0, .. }) if label == "Weekly"
        ));
    }
}
