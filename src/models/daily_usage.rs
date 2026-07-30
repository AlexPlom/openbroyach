use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyUsageEntry {
    pub date: String,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyUsageSeries {
    pub daily: Vec<DailyUsageEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsageEntry {
    pub model: String,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
    pub variants: Option<Vec<ModelUsageVariant>>,
}

impl ModelUsageEntry {
    pub const UNATTRIBUTED: &'static str = "Unattributed";
    pub const OTHER: &'static str = "Other";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsageVariant {
    pub model: String,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyModelUsageEntry {
    pub date: String,
    pub models: Vec<ModelUsageEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsageSeries {
    pub daily: Vec<DailyModelUsageEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageHistory {
    pub series: DailyUsageSeries,
    pub model_usage: Option<ModelUsageSeries>,
    pub unknown_models_by_day: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsageBreakdown {
    pub total_tokens: i64,
    pub total_cost_usd: Option<f64>,
    pub models: Vec<ModelUsageEntry>,
    pub source_note: String,
}

#[derive(Debug, Clone)]
pub struct LogUsageScan {
    pub series: DailyUsageSeries,
    pub model_usage: Option<ModelUsageSeries>,
    pub unknown_models_by_day: HashMap<String, HashSet<String>>,
}

pub const USAGE_HISTORY_DAYS: i64 = 30;

/// Accumulates daily token/cost data from log scanners
pub struct DailyUsageAccumulator;

impl DailyUsageAccumulator {
    pub fn day_key_from_date(dt: chrono::DateTime<chrono::Utc>) -> String {
        dt.format("%Y-%m-%d").to_string()
    }
}
