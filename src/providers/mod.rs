pub mod antigravity;
pub mod codex;
pub mod opencode;

use crate::models::MetricLine;

/// Shared error categories matching the Swift ErrorCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    NotLoggedIn,
    AuthExpired,
    AuthInvalid,
    CredentialAccess,
    Network,
    Decoding,
    Http4xx,
    Http5xx,
    RateLimited,
    NotAvailable,
    Other,
}

impl ErrorCategory {
    pub fn from_status(status: u16) -> Self {
        match status {
            429 => ErrorCategory::RateLimited,
            400..=499 => ErrorCategory::Http4xx,
            500..=599 => ErrorCategory::Http5xx,
            _ => ErrorCategory::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::NotLoggedIn => "not_logged_in",
            ErrorCategory::AuthExpired => "auth_expired",
            ErrorCategory::AuthInvalid => "auth_invalid",
            ErrorCategory::CredentialAccess => "credential_access",
            ErrorCategory::Network => "network",
            ErrorCategory::Decoding => "decoding",
            ErrorCategory::Http4xx => "http_4xx",
            ErrorCategory::Http5xx => "http_5xx",
            ErrorCategory::RateLimited => "rate_limited",
            ErrorCategory::NotAvailable => "not_available",
            ErrorCategory::Other => "other",
        }
    }
}

/// Shared spend tile mapper — turns daily token/cost data into Today/Yesterday/Last30 lines.
pub mod spend_tile_mapper {
    use crate::models::*;
    use chrono::Utc;

    pub fn append_token_usage(
        usage: &DailyUsageSeries,
        lines: &mut Vec<MetricLine>,
        now: chrono::DateTime<Utc>,
        estimated: bool,
        unknown_models_by_day: &std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        >,
        _model_usage: Option<&ModelUsageSeries>,
        _source_note: Option<&str>,
    ) {
        let today = DailyUsageAccumulator::day_key_from_date(now);
        let yesterday = DailyUsageAccumulator::day_key_from_date(now - chrono::Duration::days(1));

        if let Some(entry) = usage.daily.iter().find(|e| e.date == today) {
            if has_usage(entry) {
                lines.push(MetricLine::Values {
                    label: "Today".to_string(),
                    values: spend_values(entry.total_tokens, entry.cost_usd, estimated),
                    color_hex: None,
                    expiries_at: vec![],
                    unknown_models: unknown_models_by_day
                        .get(&today)
                        .map(|s| {
                            let mut v: Vec<_> = s.iter().cloned().collect();
                            v.sort();
                            v
                        })
                        .unwrap_or_default(),
                    model_breakdown: None,
                });
            }
        }

        if let Some(entry) = usage.daily.iter().find(|e| e.date == yesterday) {
            if has_usage(entry) {
                lines.push(MetricLine::Values {
                    label: "Yesterday".to_string(),
                    values: spend_values(entry.total_tokens, entry.cost_usd, estimated),
                    color_hex: None,
                    expiries_at: vec![],
                    unknown_models: unknown_models_by_day
                        .get(&yesterday)
                        .map(|s| {
                            let mut v: Vec<_> = s.iter().cloned().collect();
                            v.sort();
                            v
                        })
                        .unwrap_or_default(),
                    model_breakdown: None,
                });
            }
        }

        let total_tokens: i64 = usage.daily.iter().map(|e| e.total_tokens).sum();
        let cost_samples: Vec<f64> = usage.daily.iter().filter_map(|e| e.cost_usd).collect();
        let total_cost = if cost_samples.is_empty() {
            None
        } else {
            Some(cost_samples.iter().sum::<f64>())
        };
        if total_tokens > 0 || total_cost.unwrap_or(0.0) > 0.0 {
            let all_unknown: std::collections::HashSet<String> = unknown_models_by_day
                .values()
                .flat_map(|s| s.iter().cloned())
                .collect();
            let mut sorted: Vec<_> = all_unknown.into_iter().collect();
            sorted.sort();
            lines.push(MetricLine::Values {
                label: "Last 30 Days".to_string(),
                values: spend_values(total_tokens, total_cost, estimated),
                color_hex: None,
                expiries_at: vec![],
                unknown_models: sorted,
                model_breakdown: None,
            });
        }
    }

    fn has_usage(entry: &DailyUsageEntry) -> bool {
        entry.total_tokens > 0 || entry.cost_usd.unwrap_or(0.0) > 0.0
    }

    fn spend_values(tokens: i64, cost_usd: Option<f64>, estimated: bool) -> Vec<MetricValue> {
        let mut values = Vec::new();
        if let Some(cost) = cost_usd {
            values.push(MetricValue {
                number: cost,
                kind: MetricKind::Dollars,
                label: None,
                estimated,
            });
        }
        values.push(MetricValue::with_label(
            tokens as f64,
            MetricKind::Count,
            "tokens",
        ));
        values
    }
}

/// Read credentials from environment variables or config files
pub fn read_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn read_json_config(
    path: &std::path::Path,
) -> Result<Option<serde_json::Value>, std::io::Error> {
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    } else {
        Ok(None)
    }
}

pub fn expand_home(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Append no-usage-data badge when no lines exist
pub fn append_no_data_if_needed(lines: &mut Vec<MetricLine>) {
    if lines.is_empty() {
        lines.push(MetricLine::Badge {
            label: "Status".to_string(),
            text: "No usage data".to_string(),
            color_hex: Some("#A3A3A3".to_string()),
            subtitle: None,
        });
    }
}
