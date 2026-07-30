use crate::http::{HttpClient, HttpRequest, HttpResponse};

pub struct CodexUsageClient {
    http: HttpClient,
}

impl CodexUsageClient {
    pub const CLIENT_ID: &'static str = "app_EMoamEEZ73f0CkXaXp7hrann";
    pub const REFRESH_URL: &'static str = "https://auth.openai.com/oauth/token";
    pub const USAGE_URL: &'static str = "https://chatgpt.com/backend-api/wham/usage";
    pub const RESET_CREDITS_URL: &'static str =
        "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
    pub const CONSUME_RESET_URL: &'static str =
        "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";

    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<CodexRefreshResponse, CodexClientError> {
        let body = format!(
            "grant_type=refresh_token&client_id={}&refresh_token={}",
            Self::CLIENT_ID,
            urlencoding(refresh_token)
        );

        let response = self
            .http
            .send(HttpRequest {
                method: "POST".to_string(),
                url: Self::REFRESH_URL.to_string(),
                headers: [(
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                )]
                .into(),
                body: Some(body.into_bytes()),
                timeout: std::time::Duration::from_secs(15),
            })
            .await
            .map_err(|_e| CodexClientError::RequestFailed(0))?;

        if response.status_code == 400 || response.status_code == 401 {
            let json = response.body_json();
            let code = json
                .as_ref()
                .and_then(|j| j.get("error").and_then(|e| e.as_str()))
                .or_else(|| {
                    json.as_ref()
                        .and_then(|j| j.get("code").and_then(|c| c.as_str()))
                });

            return match code {
                Some("refresh_token_expired") => Err(CodexClientError::SessionExpired),
                Some("refresh_token_reused") => Err(CodexClientError::TokenConflict),
                Some("refresh_token_invalidated") => Err(CodexClientError::TokenRevoked),
                _ => Err(CodexClientError::RequestFailed(response.status_code)),
            };
        }

        if !(200..300).contains(&response.status_code) {
            return Err(CodexClientError::RequestFailed(response.status_code));
        }

        let json = response
            .body_json()
            .ok_or(CodexClientError::InvalidResponse)?;
        let access_token = json
            .get("access_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or(CodexClientError::TokenExpired)?;

        Ok(CodexRefreshResponse {
            access_token: access_token.to_string(),
            refresh_token: json
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .map(String::from),
            id_token: json
                .get("id_token")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    }

    pub async fn fetch_usage(
        &self,
        access_token: &str,
        account_id: Option<&str>,
    ) -> Result<HttpResponse, CodexClientError> {
        let mut headers = vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", access_token),
            ),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        if let Some(aid) = account_id {
            headers.push(("ChatGPT-Account-Id".to_string(), aid.to_string()));
        }
        self.http
            .send(HttpRequest {
                method: "GET".to_string(),
                url: Self::USAGE_URL.to_string(),
                headers: headers.into_iter().collect(),
                body: None,
                timeout: std::time::Duration::from_secs(10),
            })
            .await
            .map_err(|_e| CodexClientError::RequestFailed(0))
    }

    pub async fn fetch_reset_credits(
        &self,
        access_token: &str,
        account_id: Option<&str>,
    ) -> Result<HttpResponse, CodexClientError> {
        let mut headers = vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", access_token),
            ),
            ("Accept".to_string(), "application/json".to_string()),
            ("OpenAI-Beta".to_string(), "codex-1".to_string()),
            ("originator".to_string(), "Codex Desktop".to_string()),
        ];
        if let Some(aid) = account_id {
            headers.push(("ChatGPT-Account-Id".to_string(), aid.to_string()));
        }
        self.http
            .send(HttpRequest {
                method: "GET".to_string(),
                url: Self::RESET_CREDITS_URL.to_string(),
                headers: headers.into_iter().collect(),
                body: None,
                timeout: std::time::Duration::from_secs(10),
            })
            .await
            .map_err(|_e| CodexClientError::RequestFailed(0))
    }
}

#[derive(Debug, Clone)]
pub struct CodexRefreshResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexClientError {
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("Session expired")]
    SessionExpired,
    #[error("Token conflict")]
    TokenConflict,
    #[error("Token revoked")]
    TokenRevoked,
    #[error("Token expired")]
    TokenExpired,
    #[error("Invalid response")]
    InvalidResponse,
    #[error("Request failed: {0}")]
    RequestFailed(u16),
}

fn urlencoding(s: &str) -> String {
    urlencoding_internal(s)
}

fn urlencoding_internal(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|&c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (c as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", c),
        })
        .collect()
}
