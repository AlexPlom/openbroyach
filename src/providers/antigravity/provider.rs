use super::auth::{AntigravityAuthError, AntigravityAuthStore};
use super::client::{AntigravityUsageClient, CloudCodeOutcome, TokenRefreshOutcome};
use super::mapper::AntigravityUsageMapper;
use crate::http::HttpClient;
use crate::models::{MetricLine, Provider, ProviderSnapshot, WidgetDescriptor};
use crate::providers::ErrorCategory;
use std::collections::HashMap;

pub struct AntigravityProvider {
    pub provider: Provider,
    pub auth_store: AntigravityAuthStore,
    pub usage_client: AntigravityUsageClient,
}

impl AntigravityProvider {
    pub fn new(http: HttpClient) -> Self {
        Self {
            provider: Provider::new("antigravity", "Antigravity"),
            auth_store: AntigravityAuthStore::new(),
            usage_client: AntigravityUsageClient::new(http),
        }
    }

    pub fn widget_descriptors(&self) -> Vec<WidgetDescriptor> {
        vec![
            WidgetDescriptor::percent("antigravity.geminiPro", "antigravity", "Session"),
            WidgetDescriptor::percent("antigravity.geminiWeekly", "antigravity", "Weekly"),
            WidgetDescriptor::percent("antigravity.claude", "antigravity", "Claude"),
            WidgetDescriptor::percent("antigravity.claudeWeekly", "antigravity", "Claude Weekly"),
        ]
    }

    pub fn has_local_credentials(&self) -> bool {
        match self.auth_store.load_os_token() {
            Ok(Some(_)) => true,
            Ok(None) => {
                self.auth_store.discard_cached_token();
                false
            }
            Err(_) => true,
        }
    }

    pub async fn refresh(&self) -> ProviderSnapshot {
        match self.probe_cloud_code().await {
            Ok(result) => ProviderSnapshot::make(
                &self.provider,
                result.plan.as_deref(),
                result.lines,
                chrono::Utc::now().timestamp_millis(),
                None,
                None,
            ),
            Err(error) => error_snapshot(&self.provider, error),
        }
    }

    async fn probe_cloud_code(&self) -> Result<StrategyResult, AntigravityError> {
        let keychain_token = self
            .auth_store
            .load_os_token()
            .map_err(AntigravityError::from)?;
        let Some(keychain_token) = keychain_token else {
            self.auth_store.discard_cached_token();
            return Err(AntigravityError::NotSignedIn);
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut tokens = Vec::new();
        if keychain_token.access_is_usable(now_ms) {
            if let Some(access_token) = keychain_token.access_token.clone() {
                tokens.push(access_token);
            }
        }
        if let Some(cached) = self.auth_store.load_cached_token(&keychain_token, now_ms) {
            if !tokens.contains(&cached) {
                tokens.push(cached);
            }
        }
        let has_refresh = keychain_token
            .refresh_token
            .as_deref()
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false);
        let has_credentials = !tokens.is_empty() || has_refresh;
        let mut saw_auth_failure = false;

        for token in &tokens {
            match self.fetch_cloud_code(token).await {
                CloudCodeProbe::Success(result) => return Ok(result),
                CloudCodeProbe::AuthFailed => saw_auth_failure = true,
                CloudCodeProbe::Unavailable => {}
            }
        }

        if should_attempt_refresh(saw_auth_failure, tokens.is_empty(), has_refresh) {
            let refresh_token = keychain_token.refresh_token.as_deref().unwrap_or_default();
            match self.usage_client.refresh_google_token(refresh_token).await {
                TokenRefreshOutcome::Refreshed {
                    access_token,
                    expires_in,
                } => {
                    self.auth_store
                        .cache_token(&access_token, expires_in, refresh_token, now_ms);
                    return match self.fetch_cloud_code(&access_token).await {
                        CloudCodeProbe::Success(result) => Ok(result),
                        CloudCodeProbe::AuthFailed => Err(AntigravityError::AuthExpired),
                        CloudCodeProbe::Unavailable => Err(AntigravityError::Unavailable),
                    };
                }
                TokenRefreshOutcome::AuthFailed => return Err(AntigravityError::AuthExpired),
                TokenRefreshOutcome::Unavailable => return Err(AntigravityError::Unavailable),
            }
        }

        if saw_auth_failure {
            Err(AntigravityError::AuthExpired)
        } else if has_credentials {
            Err(AntigravityError::Unavailable)
        } else {
            Err(AntigravityError::NotSignedIn)
        }
    }

    async fn fetch_cloud_code(&self, token: &str) -> CloudCodeProbe {
        match self
            .usage_client
            .cloud_code(
                AntigravityUsageClient::QUOTA_SUMMARY_PATH,
                token,
                "antigravity",
                HashMap::new(),
            )
            .await
        {
            CloudCodeOutcome::AuthFailed => return CloudCodeProbe::AuthFailed,
            CloudCodeOutcome::Ok(data) => {
                if let Some(lines) = AntigravityUsageMapper::parse_quota_summary(&data) {
                    return CloudCodeProbe::Success(StrategyResult {
                        plan: self.load_plan(token).await,
                        lines,
                    });
                }
            }
            CloudCodeOutcome::Unavailable => {}
        }

        match self
            .usage_client
            .cloud_code(
                AntigravityUsageClient::FETCH_MODELS_PATH,
                token,
                "antigravity",
                HashMap::new(),
            )
            .await
        {
            CloudCodeOutcome::AuthFailed => return CloudCodeProbe::AuthFailed,
            CloudCodeOutcome::Ok(data) => {
                let lines = AntigravityUsageMapper::build_lines(
                    &AntigravityUsageMapper::parse_cloud_code_models(&data),
                );
                if !lines.is_empty() {
                    return CloudCodeProbe::Success(StrategyResult {
                        plan: self.load_plan(token).await,
                        lines,
                    });
                }
            }
            CloudCodeOutcome::Unavailable => {}
        }

        let mut plan = None;
        let mut project = None;
        match self
            .usage_client
            .cloud_code(
                AntigravityUsageClient::LOAD_CODE_ASSIST_PATH,
                token,
                "agy",
                HashMap::new(),
            )
            .await
        {
            CloudCodeOutcome::AuthFailed => return CloudCodeProbe::AuthFailed,
            CloudCodeOutcome::Ok(data) => {
                plan = AntigravityUsageMapper::parse_load_code_assist_plan(&data);
                project = AntigravityUsageMapper::parse_project(&data);
            }
            CloudCodeOutcome::Unavailable => {}
        }

        let quota_body = project
            .as_ref()
            .map(|project| HashMap::from([("project".to_string(), project.clone())]))
            .unwrap_or_default();
        let mut quota = self
            .usage_client
            .cloud_code(
                AntigravityUsageClient::RETRIEVE_QUOTA_PATH,
                token,
                "agy",
                quota_body,
            )
            .await;
        if matches!(quota, CloudCodeOutcome::Unavailable) && project.is_some() {
            quota = self
                .usage_client
                .cloud_code(
                    AntigravityUsageClient::RETRIEVE_QUOTA_PATH,
                    token,
                    "agy",
                    HashMap::new(),
                )
                .await;
        }
        match quota {
            CloudCodeOutcome::AuthFailed => CloudCodeProbe::AuthFailed,
            CloudCodeOutcome::Ok(data) => {
                let lines = AntigravityUsageMapper::build_lines(
                    &AntigravityUsageMapper::parse_quota_buckets(&data),
                );
                if lines.is_empty() {
                    CloudCodeProbe::Unavailable
                } else {
                    CloudCodeProbe::Success(StrategyResult { plan, lines })
                }
            }
            CloudCodeOutcome::Unavailable => CloudCodeProbe::Unavailable,
        }
    }

    async fn load_plan(&self, token: &str) -> Option<String> {
        match self
            .usage_client
            .cloud_code(
                AntigravityUsageClient::LOAD_CODE_ASSIST_PATH,
                token,
                "agy",
                HashMap::new(),
            )
            .await
        {
            CloudCodeOutcome::Ok(data) => {
                AntigravityUsageMapper::parse_load_code_assist_plan(&data)
            }
            CloudCodeOutcome::AuthFailed | CloudCodeOutcome::Unavailable => None,
        }
    }
}

struct StrategyResult {
    plan: Option<String>,
    lines: Vec<MetricLine>,
}

enum CloudCodeProbe {
    Success(StrategyResult),
    AuthFailed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AntigravityError {
    NotSignedIn,
    CredentialStoreUnreadable,
    InvalidCredentialData,
    AuthExpired,
    Unavailable,
}

impl From<AntigravityAuthError> for AntigravityError {
    fn from(value: AntigravityAuthError) -> Self {
        match value {
            AntigravityAuthError::CredentialStoreUnreadable => Self::CredentialStoreUnreadable,
            AntigravityAuthError::InvalidCredentialData => Self::InvalidCredentialData,
        }
    }
}

fn should_attempt_refresh(saw_auth_failure: bool, tokens_empty: bool, has_refresh: bool) -> bool {
    (saw_auth_failure || tokens_empty) && has_refresh
}

fn error_snapshot(provider: &Provider, error: AntigravityError) -> ProviderSnapshot {
    let (message, category) = match error {
        AntigravityError::NotSignedIn => (
            "Start Antigravity or run `agy` and try again.",
            ErrorCategory::NotLoggedIn,
        ),
        AntigravityError::CredentialStoreUnreadable => (
            "Couldn't read Antigravity credentials from the system credential store. Unlock it or sign in to Antigravity again.",
            ErrorCategory::CredentialAccess,
        ),
        AntigravityError::InvalidCredentialData => (
            "Antigravity credentials are invalid. Open Antigravity or run `agy` to sign in again.",
            ErrorCategory::AuthInvalid,
        ),
        AntigravityError::AuthExpired => (
            "Antigravity sign-in expired. Open Antigravity or run `agy` to refresh.",
            ErrorCategory::AuthExpired,
        ),
        AntigravityError::Unavailable => (
            "Antigravity usage is temporarily unavailable. Try again shortly.",
            ErrorCategory::Network,
        ),
    };
    ProviderSnapshot::error(provider, message, Some(category.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_helper_requires_auth_evidence_or_no_token() {
        assert!(should_attempt_refresh(true, false, true));
        assert!(should_attempt_refresh(false, true, true));
        assert!(!should_attempt_refresh(false, false, true));
        assert!(!should_attempt_refresh(true, false, false));
    }

    #[test]
    fn friendly_errors_match_native_messages_and_categories() {
        let provider = Provider::new("antigravity", "Antigravity");
        let cases = [
            (AntigravityError::NotSignedIn, "Start Antigravity or run `agy` and try again.", "not_logged_in"),
            (AntigravityError::CredentialStoreUnreadable, "Couldn't read Antigravity credentials from the system credential store. Unlock it or sign in to Antigravity again.", "credential_access"),
            (AntigravityError::InvalidCredentialData, "Antigravity credentials are invalid. Open Antigravity or run `agy` to sign in again.", "auth_invalid"),
            (AntigravityError::AuthExpired, "Antigravity sign-in expired. Open Antigravity or run `agy` to refresh.", "auth_expired"),
            (AntigravityError::Unavailable, "Antigravity usage is temporarily unavailable. Try again shortly.", "network"),
        ];
        for (error, message, category) in cases {
            let snapshot = error_snapshot(&provider, error);
            assert_eq!(snapshot.error_message(), Some(message));
            assert_eq!(snapshot.error_category.as_deref(), Some(category));
        }
    }

    #[test]
    fn provider_has_no_invented_links_and_exact_descriptor_order() {
        let provider = AntigravityProvider::new(HttpClient::new());
        assert!(provider.provider.links.is_empty());
        assert_eq!(
            provider
                .widget_descriptors()
                .iter()
                .map(|descriptor| descriptor.metric_label.as_str())
                .collect::<Vec<_>>(),
            ["Session", "Weekly", "Claude", "Claude Weekly"]
        );
    }
}
