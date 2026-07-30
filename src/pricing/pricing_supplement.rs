use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const PRICING_SUPPLEMENT_JSON: &str = include_str!("../../pricing_data/pricing_supplement.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingSupplement {
    pub updated_at: String,
    #[serde(default)]
    pub pricing: HashMap<String, SupplementEntry>,
    #[serde(default)]
    pub alias_rules: Vec<AliasRuleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplementEntry {
    pub input_per_million: f64,
    pub output_per_million: f64,
    #[serde(default)]
    pub cache_read_per_million: Option<f64>,
    #[serde(default)]
    pub fast_multiplier: f64,
    #[serde(default)]
    pub cache_read_is_explicit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasRuleEntry {
    pub pattern: String,
    #[serde(rename = "canonical")]
    pub alias_of: String,
    #[serde(default)]
    pub delete_prefix: Option<String>,
    #[serde(default)]
    pub fast: Option<f64>,
}

impl PricingSupplement {
    pub fn canonical_name(&self, model: &str) -> Option<String> {
        for entry in &self.alias_rules {
            if let Some(prefix) = &entry.delete_prefix {
                if model == entry.pattern
                    || model
                        .strip_prefix(prefix)
                        .is_some_and(|value| !value.is_empty())
                {
                    return Some(entry.alias_of.clone());
                }
            }
            if regex_lite::Regex::new(&entry.pattern)
                .ok()
                .is_some_and(|pattern| pattern.is_match(model))
            {
                return Some(entry.alias_of.clone());
            }
        }
        None
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let mut supplement: Self = serde_json::from_str(PRICING_SUPPLEMENT_JSON)?;
        for entry in supplement.pricing.values_mut() {
            entry.cache_read_is_explicit = entry.cache_read_per_million.is_some();
        }
        Ok(supplement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_rules_match_logged_model_variants_as_regular_expressions() {
        let supplement = PricingSupplement::load().unwrap();

        assert_eq!(
            supplement.canonical_name("gpt-5.4-high"),
            Some("gpt-5.4".to_string())
        );
        assert_eq!(
            supplement.canonical_name("gpt-5.6-sol-xhigh-fast"),
            Some("gpt-5.6-sol-fast".to_string())
        );
    }

    #[test]
    fn loaded_cache_rates_are_marked_as_explicit() {
        let supplement = PricingSupplement::load().unwrap();
        let entry = supplement.pricing.get("gpt-5.6-sol").unwrap();

        assert!(entry.cache_read_per_million.is_some());
        assert!(entry.cache_read_is_explicit);
    }
}
