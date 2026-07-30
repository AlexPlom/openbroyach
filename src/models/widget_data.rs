use super::{MetricChartPoint, MetricKind, MetricValue, ModelUsageBreakdown, ValueSelection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetData {
    pub title: String,
    pub kind: MetricKind,
    pub used: f64,
    pub limit: Option<f64>,
    pub count_suffix: Option<String>,
    pub resets_at: Option<i64>,
    pub expiries_at: Vec<i64>,
    pub period_duration_ms: Option<i64>,
    pub unknown_models: Vec<String>,
    pub model_breakdown: Option<ModelUsageBreakdown>,
    pub has_data: bool,
    pub values: Vec<MetricValue>,
    pub selection: ValueSelection,
    pub is_usage_period: bool,
    pub is_chart: bool,
    pub chart_points: Vec<MetricChartPoint>,
    pub subtitle: Option<String>,
    pub unbounded_value_word: Option<String>,
    pub info_note: Option<String>,
    pub is_session_window: bool,
    pub shows_reset_expiries: bool,
}

impl WidgetData {
    pub fn is_bounded(&self) -> bool {
        self.limit.is_some()
    }

    pub fn selected_values(&self) -> Vec<MetricValue> {
        self.selection.apply(&self.values)
    }

    pub fn value_text(&self) -> String {
        if !self.has_data {
            return "—".to_string();
        }
        let selected = self.selected_values();
        if let Some(first) = selected.first() {
            format_number(first.number, first.kind)
        } else {
            format_number(self.used, self.kind)
        }
    }
}

fn format_number(n: f64, kind: MetricKind) -> String {
    match kind {
        MetricKind::Dollars => format!("${:.2}", n),
        MetricKind::Percent => format!("{:.0}%", n),
        MetricKind::Count => {
            if n >= 1_000_000_000.0 {
                format!("{:.1}B", n / 1_000_000_000.0)
            } else if n >= 1_000_000.0 {
                format!("{:.1}M", n / 1_000_000.0)
            } else if n >= 1_000.0 {
                format!("{:.1}K", n / 1_000.0)
            } else if n.fract() == 0.0 {
                format!("{:.0}", n)
            } else {
                format!("{:.1}", n)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetDescriptor {
    pub id: String,
    pub provider_id: String,
    pub metric_label: String,
    pub sample: WidgetData,
    pub pinnable: bool,
    pub is_spend_tile: bool,
}

impl WidgetDescriptor {
    pub fn title(&self) -> &str {
        &self.sample.title
    }
}

impl WidgetDescriptor {
    pub fn percent(id: &str, provider_id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            metric_label: title.to_string(),
            sample: WidgetData {
                title: title.to_string(),
                kind: MetricKind::Percent,
                used: 0.0,
                limit: Some(100.0),
                count_suffix: None,
                resets_at: None,
                expiries_at: vec![],
                period_duration_ms: None,
                unknown_models: vec![],
                model_breakdown: None,
                has_data: false,
                values: vec![],
                selection: ValueSelection::All,
                is_usage_period: false,
                is_chart: false,
                chart_points: vec![],
                subtitle: None,
                unbounded_value_word: None,
                info_note: None,
                is_session_window: false,
                shows_reset_expiries: false,
            },
            pinnable: true,
            is_spend_tile: false,
        }
    }

    pub fn bounded_dollars(id: &str, provider_id: &str, title: &str, limit: f64) -> Self {
        Self {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            metric_label: title.to_string(),
            sample: WidgetData {
                title: title.to_string(),
                kind: MetricKind::Dollars,
                used: 0.0,
                limit: Some(limit),
                ..Default::default()
            },
            pinnable: true,
            is_spend_tile: false,
        }
    }

    pub fn values(
        id: &str,
        provider_id: &str,
        title: &str,
        selection: ValueSelection,
        value_word: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            metric_label: title.to_string(),
            sample: WidgetData {
                title: title.to_string(),
                kind: if matches!(selection, ValueSelection::Kind(MetricKind::Count)) {
                    MetricKind::Count
                } else {
                    MetricKind::Dollars
                },
                used: 0.0,
                limit: None,
                unbounded_value_word: value_word.map(String::from),
                selection,
                ..Default::default()
            },
            pinnable: true,
            is_spend_tile: false,
        }
    }

    pub fn spend_tiles(provider_id: &str) -> Vec<Self> {
        vec![
            Self::combined(&format!("{}.today", provider_id), provider_id, "Today"),
            Self::combined(
                &format!("{}.yesterday", provider_id),
                provider_id,
                "Yesterday",
            ),
            Self::combined(
                &format!("{}.last30", provider_id),
                provider_id,
                "Last 30 Days",
            ),
        ]
    }

    pub fn combined(id: &str, provider_id: &str, title: &str) -> Self {
        let mut d = Self::values(id, provider_id, title, ValueSelection::All, None);
        d.sample.is_usage_period = true;
        d.is_spend_tile = true;
        d
    }
}

impl Default for WidgetData {
    fn default() -> Self {
        Self {
            title: String::new(),
            kind: MetricKind::Count,
            used: 0.0,
            limit: None,
            count_suffix: None,
            resets_at: None,
            expiries_at: vec![],
            period_duration_ms: None,
            unknown_models: vec![],
            model_breakdown: None,
            has_data: false,
            values: vec![],
            selection: ValueSelection::All,
            is_usage_period: false,
            is_chart: false,
            chart_points: vec![],
            subtitle: None,
            unbounded_value_word: None,
            info_note: None,
            is_session_window: false,
            shows_reset_expiries: false,
        }
    }
}
