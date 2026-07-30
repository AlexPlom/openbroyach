use crate::http::{HttpClient, HttpRequest, HttpResponse};

pub struct ClaudeUsageClient {
    http: HttpClient,
}

impl ClaudeUsageClient {
    const USAGE_URL: &'static str = "https://api.anthropic.com/api/oauth/usage";

    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn fetch(&self, token: &str) -> Result<HttpResponse, reqwest::Error> {
        self.http
            .send(HttpRequest {
                method: "GET".to_string(),
                url: Self::USAGE_URL.to_string(),
                headers: [
                    ("Authorization".to_string(), format!("Bearer {token}")),
                    ("Accept".to_string(), "application/json".to_string()),
                    ("anthropic-beta".to_string(), "oauth-2025-04-20".to_string()),
                ]
                .into(),
                body: None,
                timeout: std::time::Duration::from_secs(10),
            })
            .await
    }
}
