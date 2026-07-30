use super::ModelRates;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const LITELLM_SNAPSHOT_JSON: &str =
    include_str!("../../pricing_data/pricing_litellm_snapshot.json");
const MODELS_DEV_SNAPSHOT_JSON: &str =
    include_str!("../../pricing_data/pricing_models_dev_snapshot.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingCatalog {
    #[serde(flatten)]
    pub entries: HashMap<String, FlatEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatEntry {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
}

impl PricingCatalog {
    pub fn resolve(&self, model: &str) -> Option<ModelRates> {
        self.entries.get(model).map(|entry| {
            let mut rates = ModelRates::default();
            rates.input_per_million = entry.input;
            rates.output_per_million = entry.output;
            rates.cache_read_per_million = entry.cache_read;
            rates.cache_read_is_explicit = entry.cache_read.is_some();
            rates
        })
    }

    /// Fuzzy match: trim -fast suffix, try lowercase, etc.
    pub fn fuzzy_resolve(&self, model: &str) -> Option<ModelRates> {
        // Try exact
        if let Some(r) = self.resolve(model) {
            return Some(r);
        }
        // Try with -fast stripped
        if let Some(stripped) = model.strip_suffix("-fast") {
            if let Some(r) = self.resolve(stripped) {
                let mut rates = r;
                rates.fast_multiplier = 1.5; // default fast multiplier
                return Some(rates);
            }
        }
        // Try lowercase
        let lower = model.to_lowercase();
        if let Some(r) = self.resolve(&lower) {
            return Some(r);
        }
        // Try date-stripped (remove trailing -YYYY-MM-DD or YYYYMMDD)
        let re = regex_lite::Regex::new(r"-\d{4}-\d{2}-\d{2}$").ok();
        if let Some(re) = re {
            let stripped = re.replace(model, "").to_string();
            if stripped != model {
                return self.resolve(&stripped);
            }
        }
        None
    }

    fn load_snapshot(source_name: &str, text: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let wrapper: HashMap<String, serde_json::Value> = serde_json::from_str(text)?;
        let inner = wrapper
            .get("models")
            .and_then(|v| v.as_object())
            .ok_or_else(|| format!("{source_name}: missing 'models' key"))?;
        let mut entries = HashMap::new();
        for (key, value) in inner {
            if let Some(obj) = value.as_object() {
                let input = obj.get("i").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let output = obj.get("o").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cache_read = obj.get("cr").and_then(|v| v.as_f64());
                entries.insert(
                    key.clone(),
                    FlatEntry {
                        input,
                        output,
                        cache_read,
                    },
                );
            }
        }
        Ok(PricingCatalog { entries })
    }

    pub fn load_litellm() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_snapshot("embedded LiteLLM pricing snapshot", LITELLM_SNAPSHOT_JSON)
    }

    pub fn load_models_dev() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_snapshot(
            "embedded models.dev pricing snapshot",
            MODELS_DEV_SNAPSHOT_JSON,
        )
    }
}
