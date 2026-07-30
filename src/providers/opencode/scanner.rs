use super::auth::OpenCodeAuthStore;
use super::windows::OpenCodeGoWindowMath;
use crate::models::*;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct OpenCodeUsageScan {
    pub log_scan: LogUsageScan,
    pub go_windows: Option<super::windows::OpenCodeGoWindows>,
}

pub struct OpenCodeUsageScanner;

impl OpenCodeUsageScanner {
    pub const HOSTED_PROVIDER_IDS: &'static [&'static str] = &["opencode-go", "opencode"];
    pub const GO_PROVIDER_ID: &'static str = "opencode-go";

    pub fn new() -> Self {
        Self
    }

    fn database_paths() -> Vec<String> {
        let auth_store = OpenCodeAuthStore::new();
        let data_dir = auth_store.data_directory();
        let dir = std::path::Path::new(&data_dir);
        if !dir.is_dir() {
            return vec![];
        }
        let mut files: Vec<String> = match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.path().to_string_lossy().to_string())
                .filter(|p| {
                    let name = std::path::Path::new(p)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    name.starts_with("opencode") && name.ends_with(".db")
                })
                .collect(),
            Err(_) => return vec![],
        };
        files.sort();
        files
    }

    /// Scan OpenCode SQLite databases for usage data
    pub fn scan(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<OpenCodeUsageScan>, OpenCodeScanError> {
        let paths = Self::database_paths();
        if paths.is_empty() {
            return Ok(None);
        }

        let now_ms = now.timestamp_millis();
        let cutoff_ms = now_ms - 33 * 86_400_000; // 33 days
        let tile_cutoff = now - chrono::Duration::days(30);

        let mut all_rows: Vec<ScanRow> = Vec::new();
        let mut anchor_ms: Option<f64> = None;
        let mut has_hosted = false;

        for path in &paths {
            let conn = Connection::open(path)
                .map_err(|e| OpenCodeScanError::DatabaseError(path.clone(), e.to_string()))?;

            let provider_filter = format!(
                "({})",
                Self::HOSTED_PROVIDER_IDS
                    .iter()
                    .map(|id| format!("'{}'", id))
                    .collect::<Vec<_>>()
                    .join(",")
            );

            let sql = format!(
                "SELECT json_group_array(json_array(
                    time_created,
                    json_extract(data,'$.cost'),
                    COALESCE(json_extract(data,'$.tokens.total'),0),
                    json_extract(data,'$.modelID'),
                    json_extract(data,'$.providerID')))
                FROM message
                WHERE time_created >= {}
                  AND json_valid(data)
                  AND json_extract(data,'$.role') = 'assistant'
                  AND json_extract(data,'$.providerID') IN {}
                  AND json_type(data,'$.cost') IN ('integer','real')",
                cutoff_ms, provider_filter
            );

            let json_result: String = conn
                .query_row(&sql, [], |row| row.get(0))
                .map_err(|e| OpenCodeScanError::QueryError(path.clone(), e.to_string()))?;

            let rows = Self::parse_rows(&json_result);
            all_rows.extend(rows);

            // Monthly anchor: earliest-ever Go usage
            let anchor_sql = format!(
                "SELECT MIN(time_created) FROM message
                WHERE json_valid(data)
                  AND json_extract(data,'$.role') = 'assistant'
                  AND json_extract(data,'$.providerID') = '{}'
                  AND json_type(data,'$.cost') IN ('integer','real')",
                Self::GO_PROVIDER_ID
            );
            if let Ok(min_ms) = conn.query_row(&anchor_sql, [], |row| row.get::<_, f64>(0)) {
                anchor_ms = Some(anchor_ms.map_or(min_ms, |a| a.min(min_ms)));
            }

            has_hosted = true;
        }

        if !has_hosted {
            return Ok(None);
        }

        // Build daily series for spend tiles
        let mut by_day: BTreeMap<String, (i64, f64)> = BTreeMap::new();
        let unknown: HashMap<String, HashSet<String>> = HashMap::new();

        for row in &all_rows {
            let row_date =
                chrono::DateTime::from_timestamp_millis(row.ms as i64).unwrap_or_default();
            if row_date < tile_cutoff {
                continue;
            }
            let day_key = row_date.format("%Y-%m-%d").to_string();
            let entry = by_day.entry(day_key.clone()).or_default();
            entry.0 += row.tokens;
            entry.1 += row.cost;
        }

        let daily: Vec<DailyUsageEntry> = by_day
            .into_iter()
            .map(|(date, (tokens, cost))| DailyUsageEntry {
                date,
                total_tokens: tokens,
                cost_usd: Some(cost),
            })
            .collect();

        let series = DailyUsageSeries { daily };

        // Go windows
        let go_costs: Vec<(f64, f64)> = all_rows
            .iter()
            .filter(|r| r.provider_id == Self::GO_PROVIDER_ID)
            .map(|r| (r.ms, r.cost))
            .collect();

        let auth_store = OpenCodeAuthStore::new();
        let has_go_key = auth_store.go_api_key().ok().flatten().is_some();

        let go_windows = if has_go_key || !go_costs.is_empty() {
            Some(OpenCodeGoWindowMath::compute(&go_costs, anchor_ms, now))
        } else {
            None
        };

        Ok(Some(OpenCodeUsageScan {
            log_scan: LogUsageScan {
                series,
                model_usage: None,
                unknown_models_by_day: unknown,
            },
            go_windows,
        }))
    }

    fn parse_rows(json: &str) -> Vec<ScanRow> {
        let parsed: Vec<Vec<serde_json::Value>> = serde_json::from_str(json).unwrap_or_default();
        parsed
            .iter()
            .filter_map(|entry| {
                if entry.len() < 5 {
                    return None;
                }
                let ms = entry[0].as_f64()?;
                let cost = entry[1].as_f64().filter(|c| *c >= 0.0)?;
                let tokens = (entry[2].as_f64().unwrap_or(0.0).max(0.0).min(1e15)) as i64;
                let model = entry[3].as_str().unwrap_or("").to_string();
                let provider_id = entry[4].as_str()?.to_string();
                Some(ScanRow {
                    ms,
                    cost,
                    tokens,
                    model,
                    provider_id,
                })
            })
            .collect()
    }

    pub fn has_hosted_usage(&self) -> bool {
        let paths = Self::database_paths();
        if paths.is_empty() {
            return false;
        }
        for path in &paths {
            if let Ok(conn) = Connection::open(path) {
                let provider_filter = format!(
                    "({})",
                    Self::HOSTED_PROVIDER_IDS
                        .iter()
                        .map(|id| format!("'{}'", id))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let sql = format!(
                    "SELECT 1 FROM message
                    WHERE json_valid(data)
                      AND json_extract(data,'$.role') = 'assistant'
                      AND json_extract(data,'$.providerID') IN {}
                      AND json_type(data,'$.cost') IN ('integer','real')
                    LIMIT 1",
                    provider_filter
                );
                if conn.query_row(&sql, [], |row| row.get::<_, i32>(0)).is_ok() {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
struct ScanRow {
    ms: f64,
    cost: f64,
    tokens: i64,
    model: String,
    provider_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenCodeScanError {
    #[error("Database error for {0}: {1}")]
    DatabaseError(String, String),
    #[error("Query error for {0}: {1}")]
    QueryError(String, String),
}
