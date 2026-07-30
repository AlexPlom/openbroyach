use super::auth::*;
use super::client::*;
use super::log_scanner::*;
use super::mapper::*;
use crate::http::HttpClient;
use crate::models::*;
use crate::pricing::ModelPricing;
use crate::providers::spend_tile_mapper;
use crate::providers::{append_no_data_if_needed, ErrorCategory};

pub struct CodexProvider {
    pub provider: Provider,
    pub auth_store: CodexAuthStore,
    pub usage_client: CodexUsageClient,
    pub log_scanner: CodexLogUsageScanner,
    pub pricing: ModelPricing,
}

impl CodexProvider {
    pub fn new(http: HttpClient, pricing: ModelPricing) -> Self {
        Self {
            provider: Provider::with_links(
                "codex",
                "Codex",
                vec![
                    ("Status", "https://status.openai.com/"),
                    ("Dashboard", "https://chatgpt.com/codex/settings/usage"),
                ],
            ),
            auth_store: CodexAuthStore::new(),
            usage_client: CodexUsageClient::new(http),
            log_scanner: CodexLogUsageScanner::new(),
            pricing,
        }
    }

    pub fn widget_descriptors(&self) -> Vec<WidgetDescriptor> {
        let mut descriptors = vec![
            WidgetDescriptor::percent("codex.session", "codex", "Session"),
            WidgetDescriptor::percent("codex.weekly", "codex", "Weekly"),
            WidgetDescriptor::combined("codex.credits", "codex", "Extra Usage"),
        ];
        descriptors.extend(WidgetDescriptor::spend_tiles("codex"));
        descriptors
    }

    pub fn has_local_credentials(&self) -> bool {
        let candidates = self.auth_store.load_auth_candidates();
        if candidates.iter().any(|c| c.has_usable_access_token()) {
            return true;
        }
        false
    }

    pub async fn refresh(&self) -> ProviderSnapshot {
        let now = chrono::Utc::now();

        let file_candidates = self.auth_store.load_auth_candidates();
        let mut last_auth_error: Option<String> = None;

        for candidate in &file_candidates {
            match self.probe(candidate, now).await {
                Ok(snapshot) => return snapshot,
                Err(e) => {
                    if is_auth_fallback_error(&e) {
                        last_auth_error = Some(e.to_string());
                        continue;
                    }
                    return ProviderSnapshot::error(&self.provider, &e.to_string(), None);
                }
            }
        }

        // File candidates exhausted, no keychain fallback on non-macOS
        if let Some(msg) = last_auth_error {
            return ProviderSnapshot::error(
                &self.provider,
                &msg,
                Some(ErrorCategory::AuthExpired.as_str()),
            );
        }
        ProviderSnapshot::error(
            &self.provider,
            "Not logged in. Run `codex` to authenticate.",
            Some(ErrorCategory::NotLoggedIn.as_str()),
        )
    }

    async fn probe(
        &self,
        auth_state: &CodexAuthState,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ProviderSnapshot, Box<dyn std::error::Error>> {
        let access_token = auth_state
            .auth
            .tokens
            .as_ref()
            .and_then(|t| t.access_token.as_ref())
            .filter(|t| !t.is_empty())
            .ok_or("No access token")?;

        let account_id = auth_state
            .auth
            .tokens
            .as_ref()
            .and_then(|t| t.account_id.as_ref());

        let response = self
            .usage_client
            .fetch_usage(access_token, account_id.map(String::as_str))
            .await?;
        let reset_credits = self
            .usage_client
            .fetch_reset_credits(access_token, account_id.map(String::as_str))
            .await
            .ok();
        let now_ms = now.timestamp_millis();

        let mut mapped =
            CodexUsageMapper::map_usage_response(&response, reset_credits.as_ref(), now_ms)?;

        // Local spend tiles
        if let Some(scan) = self.log_scanner.scan(30, now, &self.pricing).await? {
            spend_tile_mapper::append_token_usage(
                &scan.series,
                &mut mapped.lines,
                now,
                true,
                &scan.unknown_models_by_day,
                scan.model_usage.as_ref(),
                Some("From your Codex logs (estimated)"),
            );
        }

        append_no_data_if_needed(&mut mapped.lines);

        Ok(ProviderSnapshot {
            provider_id: self.provider.id.clone(),
            display_name: self.provider.display_name.clone(),
            account_label: CodexAuthStore::account_email(&auth_state.auth),
            plan: mapped.plan,
            lines: mapped.lines,
            refreshed_at: now_ms,
            usage_history: None,
            warning: None,
            error_category: None,
        })
    }
}

fn is_auth_fallback_error(e: &Box<dyn std::error::Error>) -> bool {
    let msg = e.to_string();
    msg.contains("Session expired")
        || msg.contains("Token conflict")
        || msg.contains("Token revoked")
        || msg.contains("Token expired")
}
