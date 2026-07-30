use super::{ModelRates, PricingCatalog, PricingSupplement};

#[derive(Clone)]
pub struct ModelPricing {
    pub supplement: PricingSupplement,
    pub litellm: PricingCatalog,
    pub models_dev: PricingCatalog,
}

impl ModelPricing {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            supplement: PricingSupplement::load()?,
            litellm: PricingCatalog::load_litellm()?,
            models_dev: PricingCatalog::load_models_dev()?,
        })
    }

    /// Resolution order: supplement (exact) -> supplement aliases -> LiteLLM exact -> fast scaling -> LiteLLM fuzzy -> models.dev
    pub fn resolve(&self, model: &str) -> Option<ModelRates> {
        // Check supplement exact
        if let Some(entry) = self.supplement.pricing.get(model) {
            return Some(ModelRates {
                input_per_million: entry.input_per_million,
                output_per_million: entry.output_per_million,
                cache_read_per_million: entry.cache_read_per_million,
                fast_multiplier: entry.fast_multiplier,
                cache_read_is_explicit: entry.cache_read_is_explicit,
                ..Default::default()
            });
        }

        // Check supplement alias -> alias_of
        if let Some(canonical) = self.supplement.canonical_name(model) {
            let is_fast = model.ends_with("-fast");
            if canonical != model {
                if let Some(r) = self.resolve(&canonical) {
                    let mut rates = r;
                    if is_fast {
                        if let Some(rule) = self
                            .supplement
                            .alias_rules
                            .iter()
                            .find(|e| e.pattern == model)
                        {
                            if let Some(fast) = rule.fast {
                                rates.fast_multiplier = fast;
                            }
                        }
                    }
                    return Some(rates);
                }
            }
        }

        // LiteLLM exact
        if let Some(r) = self.litellm.resolve(model) {
            return Some(r);
        }

        // Fast variant via LiteLLM
        if let Some(fast_model) = model.strip_suffix("-fast") {
            if let Some(base) = self.litellm.resolve(fast_model) {
                let mut rates = base.clone();
                rates.fast_multiplier = base.fast_multiplier.max(1.5);
                return Some(rates);
            }
        }

        // LiteLLM fuzzy
        if let Some(r) = self.litellm.fuzzy_resolve(model) {
            return Some(r);
        }

        // models.dev fallback
        if let Some(r) = self.models_dev.resolve(model) {
            return Some(r);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_loads_outside_the_repository_working_directory() {
        let original = std::env::current_dir().unwrap();
        let unrelated =
            std::env::temp_dir().join(format!("openbroyach-pricing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&unrelated).unwrap();
        std::env::set_current_dir(&unrelated).unwrap();

        let result = ModelPricing::load();

        std::env::set_current_dir(original).unwrap();
        std::fs::remove_dir_all(unrelated).unwrap();
        let pricing = result.unwrap();
        assert!(pricing.resolve("gpt-5.4").is_some());
    }

    #[test]
    fn self_referential_alias_does_not_recurse() {
        let pricing = ModelPricing::load().unwrap();
        let _ = pricing.resolve("auto");
    }
}
