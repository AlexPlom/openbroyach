use super::auth::ClaudeAuthStore;
use super::client::ClaudeUsageClient;
use super::mapper::ClaudeUsageMapper;
use crate::http::HttpClient;
use crate::models::{Provider, ProviderSnapshot};
use crate::providers::{append_no_data_if_needed, ErrorCategory};

pub struct ClaudeProvider {
    pub provider: Provider,
    auth: ClaudeAuthStore,
    client: ClaudeUsageClient,
}

impl ClaudeProvider {
    pub fn new(http: HttpClient) -> Self {
        Self {
            provider: Provider::with_links(
                "claude",
                "Claude Code",
                vec![("Usage", "https://claude.ai/settings/usage")],
            ),
            auth: ClaudeAuthStore,
            client: ClaudeUsageClient::new(http),
        }
    }

    pub fn has_local_credentials(&self) -> bool {
        self.auth.has_credentials()
    }

    pub async fn refresh(&self) -> ProviderSnapshot {
        let now = chrono::Utc::now().timestamp_millis();
        let credential = match self.auth.load() {
            Ok(Some(credential)) => credential,
            Ok(None) => {
                return self.error(
                    "Not logged in. Run `claude` to authenticate.",
                    ErrorCategory::NotLoggedIn,
                )
            }
            Err(error) => return self.error(&error.to_string(), ErrorCategory::CredentialAccess),
        };
        if credential.expires_at.is_some_and(|expiry| expiry <= now) {
            return self.error(
                "Claude Code session expired. Open `claude` to refresh it.",
                ErrorCategory::AuthExpired,
            );
        }
        let response = match self.client.fetch(&credential.access_token).await {
            Ok(response) => response,
            Err(_) => {
                return self.error(
                    "Could not reach Claude usage service.",
                    ErrorCategory::Network,
                )
            }
        };
        if response.status_code == 401 || response.status_code == 403 {
            return self.error(
                "Claude Code session expired. Open `claude` to refresh it.",
                ErrorCategory::AuthExpired,
            );
        }
        if !(200..300).contains(&response.status_code) {
            return self.error(
                "Claude usage request failed.",
                ErrorCategory::from_status(response.status_code),
            );
        }
        let mut lines = match ClaudeUsageMapper::map(&response) {
            Ok(lines) => lines,
            Err(error) => return self.error(&error.to_string(), ErrorCategory::Decoding),
        };
        append_no_data_if_needed(&mut lines);
        let plan = credential.subscription_type.map(format_plan);
        ProviderSnapshot::make(&self.provider, plan.as_deref(), lines, now, None, None)
    }

    fn error(&self, message: &str, category: ErrorCategory) -> ProviderSnapshot {
        ProviderSnapshot::error(&self.provider, message, Some(category.as_str()))
    }
}

fn format_plan(plan: String) -> String {
    let mut chars = plan.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or(plan)
}
