use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl HttpRequest {
    pub fn get(url: &str) -> Self {
        Self {
            method: "GET".to_string(),
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(15),
        }
    }

    pub fn post(url: &str) -> Self {
        Self {
            method: "POST".to_string(),
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(&name.to_lowercase())
    }

    pub fn body_text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    pub fn body_json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("OpenBroyach")
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub async fn send(&self, request: HttpRequest) -> Result<HttpResponse, reqwest::Error> {
        let mut req = match request.method.as_str() {
            "GET" => self.client.get(&request.url),
            "POST" => self.client.post(&request.url),
            "PUT" => self.client.put(&request.url),
            "DELETE" => self.client.delete(&request.url),
            _ => self.client.get(&request.url),
        };

        for (key, value) in &request.headers {
            req = req.header(key, value);
        }

        if let Some(body) = request.body {
            req = req.body(body);
        }

        let response = req.timeout(request.timeout).send().await?;
        let status_code = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .unwrap_or_default();

        Ok(HttpResponse {
            status_code,
            headers,
            body,
        })
    }
}
