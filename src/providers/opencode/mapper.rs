use super::windows::OpenCodeGoWindows;
use crate::models::*;

pub struct OpenCodeUsageMapper;

impl OpenCodeUsageMapper {
    pub const SESSION_CAP: f64 = 12.0;
    pub const WEEKLY_CAP: f64 = 30.0;
    pub const MONTHLY_CAP: f64 = 60.0;

    pub fn meter_lines(windows: &OpenCodeGoWindows) -> Vec<MetricLine> {
        vec![
            MetricLine::Progress {
                label: "Session".to_string(),
                used: windows.session_spend,
                limit: Self::SESSION_CAP,
                format: ProgressFormat::Dollars,
                resets_at: windows.session_resets_at,
                period_duration_ms: Some(5 * 3600 * 1000),
                color_hex: None,
            },
            MetricLine::Progress {
                label: "Weekly".to_string(),
                used: windows.weekly_spend,
                limit: Self::WEEKLY_CAP,
                format: ProgressFormat::Dollars,
                resets_at: windows.weekly_resets_at,
                period_duration_ms: Some(7 * 24 * 3600 * 1000),
                color_hex: None,
            },
            MetricLine::Progress {
                label: "Monthly".to_string(),
                used: windows.monthly_spend,
                limit: Self::MONTHLY_CAP,
                format: ProgressFormat::Dollars,
                resets_at: windows.monthly_resets_at,
                period_duration_ms: windows.monthly_period_ms,
                color_hex: None,
            },
        ]
    }
}
