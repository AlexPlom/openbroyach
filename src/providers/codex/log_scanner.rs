use crate::models::*;
use crate::pricing::{ModelPricing, ModelRates, TokenBreakdown};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct CodexLogUsageScanner;

impl CodexLogUsageScanner {
    pub fn new() -> Self {
        Self
    }

    pub async fn scan(
        &self,
        days_back: i32,
        now: chrono::DateTime<chrono::Utc>,
        pricing: &ModelPricing,
    ) -> Result<Option<LogUsageScan>, CodexLogScanError> {
        let files = self.session_files(&self.codex_homes())?;
        if files.is_empty() {
            return Ok(None);
        }

        let since_ms = (now - chrono::Duration::days(days_back as i64)).timestamp_millis();
        let mut events = Vec::new();
        for path in files {
            let data = std::fs::read_to_string(&path)
                .map_err(|error| CodexLogScanError::File(path, error.to_string()))?;
            events.extend(Self::parse_file(&data));
        }
        events.retain(|event| event.timestamp >= since_ms);
        Ok(Some(Self::aggregate(&events, pricing)))
    }

    fn codex_homes(&self) -> Vec<String> {
        if let Ok(raw) = std::env::var("CODEX_HOME") {
            let homes: Vec<String> = raw
                .split(',')
                .map(|value| crate::providers::expand_home(value.trim()))
                .filter(|value| !value.is_empty())
                .collect();
            if !homes.is_empty() {
                return homes;
            }
        }
        dirs::home_dir()
            .map(|home| home.join(".codex").to_string_lossy().into_owned())
            .into_iter()
            .collect()
    }

    fn session_files(&self, homes: &[String]) -> Result<Vec<String>, CodexLogScanError> {
        let mut files = Vec::new();
        let mut seen_roots = HashSet::new();

        for home in homes {
            let home = Path::new(home);
            let active = home.join("sessions");
            let archived = home.join("archived_sessions");
            let roots = if active.is_dir() || archived.is_dir() {
                vec![active, archived]
            } else {
                vec![home.to_path_buf()]
            };
            let mut seen_relative = HashSet::new();

            for root in roots.into_iter().filter(|root| root.is_dir()) {
                let canonical = std::fs::canonicalize(&root).map_err(|error| {
                    CodexLogScanError::Directory(root.display().to_string(), error.to_string())
                })?;
                if !seen_roots.insert(canonical.clone()) {
                    continue;
                }
                let mut discovered = Vec::new();
                Self::collect_jsonl(&canonical, &mut discovered, &mut HashSet::new())?;
                discovered.sort();
                for path in discovered {
                    let relative = path.strip_prefix(&canonical).unwrap_or(&path).to_path_buf();
                    if seen_relative.insert(relative) {
                        files.push(path.to_string_lossy().into_owned());
                    }
                }
            }
        }

        Ok(files)
    }

    fn collect_jsonl(
        directory: &Path,
        files: &mut Vec<PathBuf>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<(), CodexLogScanError> {
        let canonical = std::fs::canonicalize(directory).map_err(|error| {
            CodexLogScanError::Directory(directory.display().to_string(), error.to_string())
        })?;
        if !visited.insert(canonical.clone()) {
            return Ok(());
        }
        let entries = std::fs::read_dir(&canonical).map_err(|error| {
            CodexLogScanError::Directory(canonical.display().to_string(), error.to_string())
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                CodexLogScanError::Directory(canonical.display().to_string(), error.to_string())
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                CodexLogScanError::Directory(path.display().to_string(), error.to_string())
            })?;
            if file_type.is_dir() {
                Self::collect_jsonl(&path, files, visited)?;
            } else if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
            {
                files.push(path);
            }
        }
        Ok(())
    }

    fn parse_file(data: &str) -> Vec<CodexEvent> {
        let mut events = Vec::new();
        let mut previous_totals: Option<RawUsage> = None;
        let mut current_model: Option<String> = None;
        let mut current_tier_is_fast = false;
        let mut saw_session_meta = false;
        let mut replay_gate: Option<ChildReplayGate> = None;

        for line in data.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let Ok(object) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(object) = object.as_object() else {
                continue;
            };
            let type_ = object.get("type").and_then(Value::as_str);
            let payload = object.get("payload").and_then(Value::as_object);

            if type_ == Some("turn_context") {
                if let Some(model) = payload.and_then(Self::model_name) {
                    current_model = Some(model);
                }
                continue;
            }

            if type_ == Some("session_meta") && !saw_session_meta {
                saw_session_meta = true;
                if payload.is_some_and(Self::is_child_session_meta) {
                    replay_gate = Some(
                        Self::timestamp_ms(object)
                            .map(|timestamp| ChildReplayGate::CreatedAt(timestamp / 1_000))
                            .unwrap_or(ChildReplayGate::SelfTimed),
                    );
                }
                continue;
            }

            if type_ != Some("event_msg") {
                continue;
            }
            let Some(payload) = payload else {
                continue;
            };

            if payload.get("type").and_then(Value::as_str) == Some("thread_settings_applied") {
                if let Some(tier) = Self::service_tier(payload) {
                    current_tier_is_fast = tier == "fast" || tier == "priority";
                }
                continue;
            }

            if payload.get("type").and_then(Value::as_str) == Some("task_started") {
                if let (Some(gate), Some(started_at)) = (
                    replay_gate,
                    payload.get("started_at").and_then(Value::as_f64),
                ) {
                    let line_seconds =
                        Self::timestamp_ms(object).map(|timestamp| timestamp / 1_000);
                    if gate.cleared_by(started_at, line_seconds) {
                        replay_gate = None;
                    }
                }
                continue;
            }

            if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                continue;
            }
            let Some(timestamp) = Self::timestamp_ms(object) else {
                continue;
            };
            let info = payload.get("info").and_then(Value::as_object);
            let totals = info
                .and_then(|value| value.get("total_token_usage"))
                .and_then(Value::as_object)
                .map(RawUsage::from_json);

            if replay_gate.is_some() {
                if let Some(totals) = totals {
                    previous_totals = Some(totals);
                }
                continue;
            }
            if totals
                .as_ref()
                .zip(previous_totals.as_ref())
                .is_some_and(|(current, previous)| current == previous)
            {
                continue;
            }

            let usage = info
                .and_then(|value| value.get("last_token_usage"))
                .and_then(Value::as_object)
                .map(RawUsage::from_json)
                .or_else(|| {
                    totals
                        .as_ref()
                        .map(|value| value.subtracting(previous_totals.as_ref()))
                });
            if let Some(totals) = totals {
                previous_totals = Some(totals);
            }
            let Some(usage) = usage else {
                continue;
            };
            if usage.input == 0 && usage.cached == 0 && usage.output == 0 && usage.reasoning == 0 {
                continue;
            }

            let parsed_model =
                Self::model_name(payload).or_else(|| info.and_then(Self::model_name));
            let model = Self::resolve_model(parsed_model, &mut current_model, timestamp);
            events.push(CodexEvent {
                timestamp,
                model,
                input: usage.input.max(0),
                cached: usage.cached.max(0).min(usage.input.max(0)),
                output: usage.output.max(0),
                reasoning: usage.reasoning.max(0),
                total: usage.total.max(0),
                is_fast: current_tier_is_fast,
            });
        }

        events
    }

    fn timestamp_ms(object: &Map<String, Value>) -> Option<i64> {
        object
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).ok())
            .map(|timestamp| timestamp.timestamp_millis())
    }

    fn model_name(object: &Map<String, Value>) -> Option<String> {
        [
            object.get("model"),
            object.get("model_name"),
            object
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|metadata| metadata.get("model")),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|model| !model.is_empty())
        .map(str::to_owned)
    }

    fn resolve_model(
        parsed: Option<String>,
        current: &mut Option<String>,
        timestamp: i64,
    ) -> String {
        let mut model = if let Some(model) = parsed {
            *current = Some(model.clone());
            model
        } else if let Some(model) = current.clone() {
            model
        } else {
            *current = Some("gpt-5".to_string());
            "gpt-5".to_string()
        };
        if model == "codex-auto-review" {
            model = Self::auto_review_fallback(timestamp).to_string();
        }
        model
    }

    fn auto_review_fallback(timestamp: i64) -> &'static str {
        let date = chrono::DateTime::from_timestamp_millis(timestamp)
            .unwrap_or_default()
            .format("%Y-%m-%d")
            .to_string();
        [
            ("2026-04-23", "gpt-5.5"),
            ("2026-03-05", "gpt-5.4"),
            ("2026-02-05", "gpt-5.3-codex"),
            ("2025-12-11", "gpt-5.2-codex"),
            ("2025-11-13", "gpt-5.1-codex"),
            ("2025-09-15", "gpt-5-codex"),
            ("2025-08-07", "gpt-5"),
        ]
        .into_iter()
        .find_map(|(released_on, model)| (date.as_str() >= released_on).then_some(model))
        .unwrap_or("gpt-5")
    }

    fn service_tier(payload: &Map<String, Value>) -> Option<&str> {
        payload
            .get("thread_settings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("service_tier"))
            .or_else(|| payload.get("service_tier"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|tier| !tier.is_empty())
    }

    fn is_child_session_meta(payload: &Map<String, Value>) -> bool {
        Self::has_value(payload.get("forked_from_id"))
            || Self::has_value(payload.get("parent_thread_id"))
            || payload.get("thread_source").and_then(Value::as_str) == Some("subagent")
            || payload
                .get("source")
                .and_then(Value::as_object)
                .is_some_and(|source| Self::has_value(source.get("subagent")))
    }

    fn has_value(value: Option<&Value>) -> bool {
        match value {
            None | Some(Value::Null) => false,
            Some(Value::String(value)) => !value.trim().is_empty(),
            Some(_) => true,
        }
    }

    fn aggregate(events: &[CodexEvent], pricing: &ModelPricing) -> LogUsageScan {
        let mut seen = HashSet::new();
        let mut by_day: BTreeMap<String, (i64, f64, BTreeMap<String, (i64, f64)>)> =
            BTreeMap::new();
        let mut unknown: HashMap<String, HashSet<String>> = HashMap::new();

        for event in events {
            if !seen.insert(EventKey::from(event)) {
                continue;
            }
            let day = chrono::DateTime::from_timestamp_millis(event.timestamp)
                .unwrap_or_default()
                .format("%Y-%m-%d")
                .to_string();
            let model = event.model.trim();
            if model.is_empty() {
                continue;
            }
            let canonical = pricing
                .supplement
                .canonical_name(model)
                .unwrap_or_else(|| model.to_string());
            let fast_alias = canonical.ends_with("-fast");
            let rate_model = canonical.strip_suffix("-fast").unwrap_or(&canonical);
            let base_rates = pricing.resolve(rate_model);
            let Some(rates) = base_rates.clone().or_else(|| pricing.resolve(model)) else {
                if event.total > 0 {
                    unknown.entry(day).or_default().insert(model.to_string());
                }
                continue;
            };
            let fast_tier = if fast_alias {
                base_rates.is_some()
            } else {
                event.is_fast
            };
            let cost = Self::cost(
                &rates,
                event,
                rate_model,
                fast_tier,
                Self::codex_priority_multiplier(rate_model, &rates),
            );
            let entry = by_day.entry(day).or_default();
            entry.0 += event.total;
            entry.1 += cost;
            let model_entry = entry.2.entry(model.to_string()).or_default();
            model_entry.0 += event.total;
            model_entry.1 += cost;
        }

        let daily = by_day
            .iter()
            .map(|(date, (tokens, cost, _))| DailyUsageEntry {
                date: date.clone(),
                total_tokens: *tokens,
                cost_usd: Some(*cost),
            })
            .collect();
        let model_daily = by_day
            .into_iter()
            .map(|(date, (_, _, models))| DailyModelUsageEntry {
                date,
                models: models
                    .into_iter()
                    .map(|(model, (total_tokens, cost))| ModelUsageEntry {
                        model,
                        total_tokens,
                        cost_usd: Some(cost),
                        variants: None,
                    })
                    .collect(),
            })
            .collect();

        LogUsageScan {
            series: DailyUsageSeries { daily },
            model_usage: Some(ModelUsageSeries { daily: model_daily }),
            unknown_models_by_day: unknown,
        }
    }

    fn cost(
        rates: &ModelRates,
        event: &CodexEvent,
        model: &str,
        fast_tier: bool,
        fast_multiplier: f64,
    ) -> f64 {
        let mut rates = rates.clone();
        if let Some((input, output, cache)) = Self::codex_long_context_rates(model) {
            rates.input_above_200k_per_million = Some(input);
            rates.output_above_200k_per_million = Some(output);
            rates.cache_read_above_200k_per_million = Some(cache);
            rates.long_context_threshold_tokens = Some(272_000);
        }
        if Self::codex_model_has_no_cache_discount(model) || !rates.cache_read_is_explicit {
            rates.cache_read_per_million = Some(rates.input_per_million);
            rates.cache_read_above_200k_per_million = rates.input_above_200k_per_million;
        }
        rates.fast_multiplier = fast_multiplier;
        rates.cost_dollars(&TokenBreakdown {
            input: event.input.saturating_sub(event.cached) as u64,
            cache_read: event.cached as u64,
            output: event.output as u64,
            is_fast: fast_tier,
        })
    }

    fn codex_priority_multiplier(model: &str, rates: &ModelRates) -> f64 {
        match Self::dated_base_model(model) {
            "gpt-5.5" | "gpt-5.5-pro" => 2.5,
            "gpt-5.4" | "gpt-5.4-pro" | "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna" => 2.0,
            _ if rates.fast_multiplier == 1.0 => 2.0,
            _ => rates.fast_multiplier,
        }
    }

    fn codex_model_has_no_cache_discount(model: &str) -> bool {
        matches!(Self::dated_base_model(model), "gpt-5.4-pro" | "gpt-5.5-pro")
    }

    fn codex_long_context_rates(model: &str) -> Option<(f64, f64, f64)> {
        match Self::dated_base_model(model) {
            "gpt-5.4" => Some((5.0, 22.5, 0.5)),
            "gpt-5.4-pro" | "gpt-5.5-pro" => Some((60.0, 270.0, 60.0)),
            "gpt-5.5" | "gpt-5.6-sol" => Some((10.0, 45.0, 1.0)),
            "gpt-5.6-terra" => Some((5.0, 22.5, 0.5)),
            "gpt-5.6-luna" => Some((2.0, 9.0, 0.2)),
            _ => None,
        }
    }

    fn dated_base_model(model: &str) -> &str {
        let bytes = model.as_bytes();
        if bytes.len() >= 11 {
            let suffix = &bytes[bytes.len() - 11..];
            if suffix[0] == b'-'
                && suffix[1..].iter().enumerate().all(|(index, character)| {
                    ((index == 4 || index == 7) && *character == b'-')
                        || (index != 4 && index != 7 && character.is_ascii_digit())
                })
            {
                return model.get(..model.len() - 11).unwrap_or(model);
            }
        }
        if bytes.len() >= 9 {
            let suffix = &bytes[bytes.len() - 9..];
            if suffix[0] == b'-' && suffix[1..].iter().all(u8::is_ascii_digit) {
                return model.get(..model.len() - 9).unwrap_or(model);
            }
        }
        model
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodexLogScanError {
    #[error("Could not enumerate Codex logs at {0}: {1}")]
    Directory(String, String),
    #[error("Could not read Codex log {0}: {1}")]
    File(String, String),
}

#[derive(Clone, Copy)]
enum ChildReplayGate {
    CreatedAt(i64),
    SelfTimed,
}

impl ChildReplayGate {
    fn cleared_by(self, started_at: f64, line_seconds: Option<i64>) -> bool {
        match self {
            Self::CreatedAt(created) => started_at >= created as f64,
            Self::SelfTimed => line_seconds.is_some_and(|line| started_at >= line as f64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawUsage {
    input: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    total: i64,
}

impl RawUsage {
    fn from_json(object: &Map<String, Value>) -> Self {
        let input = Self::int_field(object, &["input_tokens", "prompt_tokens", "input"]);
        let cached = Self::int_field(
            object,
            &[
                "cached_input_tokens",
                "cache_read_input_tokens",
                "cached_tokens",
            ],
        );
        let output = Self::int_field(object, &["output_tokens", "completion_tokens", "output"]);
        let reasoning = Self::int_field(object, &["reasoning_output_tokens", "reasoning_tokens"]);
        let reported = Self::int_field(object, &["total_tokens"]);
        let recomputed = input + output + reasoning;
        Self {
            input,
            cached,
            output,
            reasoning,
            total: if reported > 0 || recomputed == 0 {
                reported
            } else {
                recomputed
            },
        }
    }

    fn int_field(object: &Map<String, Value>, keys: &[&str]) -> i64 {
        keys.iter()
            .filter_map(|key| object.get(*key))
            .find_map(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_f64().map(|value| value as i64))
            })
            .unwrap_or(0)
    }

    fn subtracting(&self, previous: Option<&Self>) -> Self {
        let previous = previous.cloned().unwrap_or(Self {
            input: 0,
            cached: 0,
            output: 0,
            reasoning: 0,
            total: 0,
        });
        Self {
            input: (self.input - previous.input).max(0),
            cached: (self.cached - previous.cached).max(0),
            output: (self.output - previous.output).max(0),
            reasoning: (self.reasoning - previous.reasoning).max(0),
            total: (self.total - previous.total).max(0),
        }
    }
}

#[derive(Debug, Clone)]
struct CodexEvent {
    timestamp: i64,
    model: String,
    input: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    total: i64,
    is_fast: bool,
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct EventKey {
    timestamp: i64,
    model: String,
    input: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    total: i64,
}

impl From<&CodexEvent> for EventKey {
    fn from(event: &CodexEvent) -> Self {
        Self {
            timestamp: event.timestamp,
            model: event.model.clone(),
            input: event.input,
            cached: event.cached,
            output: event.output,
            reasoning: event.reasoning,
            total: event.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::{FlatEntry, PricingCatalog, PricingSupplement};
    use std::fs;

    fn temporary_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!("openbroyach-codex-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn pricing(model: &str, input: f64, output: f64, cache: Option<f64>) -> ModelPricing {
        let mut entries = HashMap::new();
        entries.insert(
            model.to_string(),
            FlatEntry {
                input,
                output,
                cache_read: cache,
            },
        );
        ModelPricing {
            supplement: PricingSupplement {
                updated_at: String::new(),
                pricing: HashMap::new(),
                alias_rules: Vec::new(),
            },
            litellm: PricingCatalog { entries },
            models_dev: PricingCatalog {
                entries: HashMap::new(),
            },
        }
    }

    fn token_line(timestamp: &str, total_input: i64, last_input: Option<i64>) -> String {
        let last = last_input
            .map(|input| {
                format!(r#", "last_token_usage":{{"input_tokens":{input},"total_tokens":{input}}}"#)
            })
            .unwrap_or_default();
        format!(
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{total_input},"total_tokens":{total_input}}}{last}}}}}}}"#
        )
    }

    #[test]
    fn discovers_jsonl_files_recursively_and_falls_back_to_home() {
        let root = temporary_directory();
        let nested = root.join("nested/deeper");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("session.jsonl"), "").unwrap();
        fs::write(root.join("ignored.txt"), "").unwrap();

        let files = CodexLogUsageScanner::new()
            .session_files(&[root.to_string_lossy().into_owned()])
            .unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("session.jsonl"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_session_wins_over_archived_copy_with_same_relative_path() {
        let root = temporary_directory();
        let active = root.join("sessions/year");
        let archive = root.join("archived_sessions/year");
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&archive).unwrap();
        fs::write(active.join("same.jsonl"), "active").unwrap();
        fs::write(archive.join("same.jsonl"), "archive").unwrap();

        let files = CodexLogUsageScanner::new()
            .session_files(&[root.to_string_lossy().into_owned()])
            .unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].contains("sessions"));
        assert!(!files[0].contains("archived_sessions"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cumulative_totals_emit_per_file_deltas() {
        let data = [
            token_line("2026-07-28T10:00:00Z", 100, None),
            token_line("2026-07-28T10:01:00Z", 140, None),
        ]
        .join("\n");

        let events = CodexLogUsageScanner::parse_file(&data);

        assert_eq!(
            events.iter().map(|event| event.input).collect::<Vec<_>>(),
            vec![100, 40]
        );
        assert_eq!(
            events.iter().map(|event| event.total).collect::<Vec<_>>(),
            vec![100, 40]
        );
    }

    #[test]
    fn unchanged_totals_suppress_repeated_last_usage() {
        let data = [
            token_line("2026-07-28T10:00:00Z", 100, Some(100)),
            token_line("2026-07-28T10:01:00Z", 100, Some(100)),
        ]
        .join("\n");

        let events = CodexLogUsageScanner::parse_file(&data);

        assert_eq!(events.len(), 1);
    }

    #[test]
    fn retired_auto_review_slug_maps_by_event_date() {
        let timestamp = chrono::DateTime::parse_from_rfc3339("2026-03-06T10:00:00Z")
            .unwrap()
            .timestamp_millis();
        let mut current = None;

        assert_eq!(
            CodexLogUsageScanner::resolve_model(
                Some("codex-auto-review".to_string()),
                &mut current,
                timestamp,
            ),
            "gpt-5.4"
        );
    }

    #[test]
    fn child_replay_seeds_baseline_until_live_task_starts() {
        let data = [
            r#"{"timestamp":"2026-07-28T10:00:00Z","type":"session_meta","payload":{"forked_from_id":"parent"}}"#.to_string(),
            token_line("2026-07-28T10:00:01Z", 1000, None),
            r#"{"timestamp":"2026-07-28T10:00:02Z","type":"event_msg","payload":{"type":"task_started","started_at":1750000000}}"#.to_string(),
            r#"{"timestamp":"2026-07-28T10:00:03Z","type":"event_msg","payload":{"type":"task_started","started_at":1785232803}}"#.to_string(),
            token_line("2026-07-28T10:00:04Z", 1025, None),
        ]
        .join("\n");

        let events = CodexLogUsageScanner::parse_file(&data);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].input, 25);
    }

    #[test]
    fn duplicate_events_from_different_files_count_once() {
        let event = CodexEvent {
            timestamp: 1_785_232_800_000,
            model: "known".to_string(),
            input: 100,
            cached: 0,
            output: 0,
            reasoning: 0,
            total: 100,
            is_fast: false,
        };

        let scan = CodexLogUsageScanner::aggregate(
            &[event.clone(), event],
            &pricing("known", 1.0, 1.0, None),
        );

        assert_eq!(scan.series.daily[0].total_tokens, 100);
    }

    #[test]
    fn cached_tokens_use_explicit_cache_rate_and_not_input_rate() {
        let event = CodexEvent {
            timestamp: 1_785_232_800_000,
            model: "known".to_string(),
            input: 1_000_000,
            cached: 750_000,
            output: 0,
            reasoning: 0,
            total: 1_000_000,
            is_fast: false,
        };

        let scan =
            CodexLogUsageScanner::aggregate(&[event], &pricing("known", 4.0, 10.0, Some(1.0)));

        assert_eq!(scan.series.daily[0].cost_usd, Some(1.75));
    }

    #[test]
    fn long_context_rate_is_strictly_above_272k_and_includes_cached_input() {
        let rates = ModelRates {
            input_per_million: 1.0,
            output_per_million: 1.0,
            cache_read_per_million: Some(1.0),
            input_above_200k_per_million: Some(2.0),
            output_above_200k_per_million: Some(2.0),
            cache_read_above_200k_per_million: Some(2.0),
            long_context_threshold_tokens: Some(272_000),
            cache_read_is_explicit: true,
            ..Default::default()
        };
        let at_threshold = rates.cost_dollars(&TokenBreakdown {
            input: 200_000,
            cache_read: 72_000,
            output: 0,
            is_fast: false,
        });
        let above_threshold = rates.cost_dollars(&TokenBreakdown {
            input: 200_000,
            cache_read: 72_001,
            output: 0,
            is_fast: false,
        });

        assert!((at_threshold - 0.272).abs() < 1e-12);
        assert!((above_threshold - 0.544002).abs() < 1e-12);
    }

    #[test]
    fn unknown_models_are_reported_but_excluded_from_totals() {
        let event = CodexEvent {
            timestamp: 1_785_232_800_000,
            model: "unknown".to_string(),
            input: 10,
            cached: 0,
            output: 0,
            reasoning: 0,
            total: 10,
            is_fast: false,
        };

        let scan = CodexLogUsageScanner::aggregate(&[event], &pricing("known", 1.0, 1.0, None));

        assert!(scan.series.daily.is_empty());
        assert!(scan
            .unknown_models_by_day
            .values()
            .next()
            .unwrap()
            .contains("unknown"));
    }
}
