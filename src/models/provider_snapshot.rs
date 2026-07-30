use super::{MetricLine, Provider, ProviderUsageHistory};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub provider_id: String,
    pub display_name: String,
    #[serde(default)]
    pub account_label: Option<String>,
    pub plan: Option<String>,
    pub lines: Vec<MetricLine>,
    pub refreshed_at: i64,
    pub usage_history: Option<ProviderUsageHistory>,
    pub warning: Option<String>,
    pub error_category: Option<String>,
}

impl ProviderSnapshot {
    pub fn make(
        provider: &Provider,
        plan: Option<&str>,
        lines: Vec<MetricLine>,
        refreshed_at: i64,
        usage_history: Option<ProviderUsageHistory>,
        warning: Option<&str>,
    ) -> Self {
        Self {
            provider_id: provider.id.clone(),
            display_name: provider.display_name.clone(),
            account_label: None,
            plan: plan.map(String::from),
            lines,
            refreshed_at,
            usage_history,
            warning: warning.map(String::from),
            error_category: None,
        }
    }

    pub fn error(provider: &Provider, message: &str, category: Option<&str>) -> Self {
        Self {
            provider_id: provider.id.clone(),
            display_name: provider.display_name.clone(),
            account_label: None,
            plan: None,
            lines: vec![MetricLine::Badge {
                label: "Error".to_string(),
                text: message.to_string(),
                color_hex: Some("#EF4444".to_string()),
                subtitle: None,
            }],
            error_category: category.map(String::from),
            refreshed_at: chrono::Utc::now().timestamp_millis(),
            usage_history: None,
            warning: None,
        }
    }

    pub fn line(&self, label: &str) -> Option<&MetricLine> {
        self.lines.iter().find(|l| l.label() == label)
    }

    pub fn error_message(&self) -> Option<&str> {
        if self.lines.is_empty() || !self.lines.iter().all(MetricLine::is_error) {
            return None;
        }
        match self.lines.first()? {
            MetricLine::Badge { text, .. } => Some(text),
            _ => Some("Refresh failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_all_error_snapshots_are_refresh_failures() {
        let provider = Provider::new("test", "Test");
        let error = ProviderSnapshot::error(&provider, "Network unavailable", Some("network"));
        assert_eq!(error.error_message(), Some("Network unavailable"));

        let mixed = ProviderSnapshot::make(
            &provider,
            None,
            vec![
                MetricLine::Badge {
                    label: "Error".to_string(),
                    text: "Partial warning".to_string(),
                    color_hex: None,
                    subtitle: None,
                },
                MetricLine::Text {
                    label: "Usage".to_string(),
                    value: "Available".to_string(),
                    color_hex: None,
                    subtitle: None,
                },
            ],
            0,
            None,
            None,
        );
        assert_eq!(mixed.error_message(), None);
    }
}
