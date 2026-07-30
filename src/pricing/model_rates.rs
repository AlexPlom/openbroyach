use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRates {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_read_per_million: Option<f64>,
    pub input_above_200k_per_million: Option<f64>,
    pub output_above_200k_per_million: Option<f64>,
    pub cache_read_above_200k_per_million: Option<f64>,
    pub long_context_threshold_tokens: Option<i64>,
    pub fast_multiplier: f64,
    pub cache_read_is_explicit: bool,
}

impl ModelRates {
    pub fn cost_dollars(&self, breakdown: &TokenBreakdown) -> f64 {
        let prompt_tokens = breakdown.input.saturating_add(breakdown.cache_read);
        let input_rate = if let (Some(threshold), Some(rate)) = (
            self.long_context_threshold_tokens,
            self.input_above_200k_per_million,
        ) {
            if prompt_tokens > threshold as u64 {
                rate
            } else {
                self.input_per_million
            }
        } else {
            self.input_per_million
        };
        let output_rate = if let (Some(threshold), Some(rate)) = (
            self.long_context_threshold_tokens,
            self.output_above_200k_per_million,
        ) {
            if prompt_tokens > threshold as u64 {
                rate
            } else {
                self.output_per_million
            }
        } else {
            self.output_per_million
        };
        let cache_rate = if let (Some(threshold), Some(rate)) = (
            self.long_context_threshold_tokens,
            self.cache_read_above_200k_per_million,
        ) {
            if prompt_tokens > threshold as u64 {
                rate
            } else {
                self.cache_read_per_million.unwrap_or(input_rate)
            }
        } else {
            self.cache_read_per_million.unwrap_or(input_rate)
        };

        let multiplier = if breakdown.is_fast {
            self.fast_multiplier
        } else {
            1.0
        };

        let input_cost = (breakdown.input as f64 / 1_000_000.0) * input_rate * multiplier;
        let cache_cost = (breakdown.cache_read as f64 / 1_000_000.0) * cache_rate * multiplier;
        let output_cost = (breakdown.output as f64 / 1_000_000.0) * output_rate * multiplier;

        input_cost + cache_cost + output_cost
    }
}

impl Default for ModelRates {
    fn default() -> Self {
        Self {
            input_per_million: 0.0,
            output_per_million: 0.0,
            cache_read_per_million: None,
            input_above_200k_per_million: None,
            output_above_200k_per_million: None,
            cache_read_above_200k_per_million: None,
            long_context_threshold_tokens: None,
            fast_multiplier: 1.0,
            cache_read_is_explicit: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenBreakdown {
    pub input: u64,
    pub cache_read: u64,
    pub output: u64,
    pub is_fast: bool,
}

impl TokenBreakdown {
    pub fn new(input: u64, cache_read: u64, output: u64) -> Self {
        Self {
            input,
            cache_read,
            output,
            is_fast: false,
        }
    }
}
