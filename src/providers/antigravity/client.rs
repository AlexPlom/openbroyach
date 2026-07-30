use crate::http::{HttpClient, HttpRequest};
use std::collections::HashMap;
use std::time::Duration;

pub enum CloudCodeOutcome {
    Ok(Vec<u8>),
    AuthFailed,
    Unavailable,
}

pub enum TokenRefreshOutcome {
    Refreshed {
        access_token: String,
        expires_in: f64,
    },
    AuthFailed,
    Unavailable,
}

pub struct AntigravityUsageClient {
    http: HttpClient,
}

impl AntigravityUsageClient {
    pub const CLOUD_CODE_URLS: [&'static str; 2] = [
        "https://daily-cloudcode-pa.googleapis.com",
        "https://cloudcode-pa.googleapis.com",
    ];
    pub const FETCH_MODELS_PATH: &'static str = "/v1internal:fetchAvailableModels";
    pub const LOAD_CODE_ASSIST_PATH: &'static str = "/v1internal:loadCodeAssist";
    pub const RETRIEVE_QUOTA_PATH: &'static str = "/v1internal:retrieveUserQuota";
    pub const QUOTA_SUMMARY_PATH: &'static str = "/v1internal:retrieveUserQuotaSummary";
    pub const GOOGLE_OAUTH_URL: &'static str = "https://oauth2.googleapis.com/token";
    // These installed-app OAuth credentials ship in Antigravity itself and are therefore public.
    // They are required to refresh Antigravity's keychain token while the app is closed.
    pub const GOOGLE_CLIENT_ID: &'static str =
        "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
    pub const GOOGLE_CLIENT_SECRET: &'static str = "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";

    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn cloud_code(
        &self,
        path: &str,
        token: &str,
        user_agent: &str,
        body: HashMap<String, String>,
    ) -> CloudCodeOutcome {
        let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
        for base in Self::CLOUD_CODE_URLS {
            let response = self
                .http
                .send(HttpRequest {
                    method: "POST".to_string(),
                    url: format!("{base}{path}"),
                    headers: [
                        ("Accept".to_string(), "application/json".to_string()),
                        ("Content-Type".to_string(), "application/json".to_string()),
                        ("Authorization".to_string(), format!("Bearer {token}")),
                        ("User-Agent".to_string(), user_agent.to_string()),
                    ]
                    .into(),
                    body: Some(payload.clone()),
                    timeout: Duration::from_secs(15),
                })
                .await;
            let Ok(response) = response else {
                continue;
            };
            if response.status_code == 401 || response.status_code == 403 {
                return CloudCodeOutcome::AuthFailed;
            }
            if (200..300).contains(&response.status_code) {
                return CloudCodeOutcome::Ok(response.body);
            }
        }
        CloudCodeOutcome::Unavailable
    }

    pub async fn refresh_google_token(&self, refresh_token: &str) -> TokenRefreshOutcome {
        let body = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            form_encode(Self::GOOGLE_CLIENT_ID),
            form_encode(Self::GOOGLE_CLIENT_SECRET),
            form_encode(refresh_token),
        );
        let response = self
            .http
            .send(HttpRequest {
                method: "POST".to_string(),
                url: Self::GOOGLE_OAUTH_URL.to_string(),
                headers: [(
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                )]
                .into(),
                body: Some(body.into_bytes()),
                timeout: Duration::from_secs(15),
            })
            .await;
        let Ok(response) = response else {
            return TokenRefreshOutcome::Unavailable;
        };
        match response.status_code {
            200..=299 => {
                let Some(json) = response.body_json() else {
                    return TokenRefreshOutcome::Unavailable;
                };
                let Some(access_token) = json
                    .get("access_token")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|token| !token.is_empty())
                else {
                    return TokenRefreshOutcome::Unavailable;
                };
                TokenRefreshOutcome::Refreshed {
                    access_token: access_token.to_string(),
                    expires_in: json
                        .get("expires_in")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(3600.0),
                }
            }
            408 | 429 => TokenRefreshOutcome::Unavailable,
            400..=499 => TokenRefreshOutcome::AuthFailed,
            _ => TokenRefreshOutcome::Unavailable,
        }
    }
}

fn form_encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|&byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::form_encode;

    #[test]
    fn oauth_form_encoding_preserves_unreserved_and_encodes_plus_and_space() {
        assert_eq!(form_encode("a+b c/~"), "a%2Bb%20c%2F~");
    }
}
