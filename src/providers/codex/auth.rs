use crate::providers::{read_env_var, read_json_config};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAuth {
    pub tokens: Option<CodexTokens>,
    pub last_refresh: Option<String>,
    pub openai_api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodexAuthState {
    pub auth: CodexAuth,
    pub source_path: Option<String>,
}

impl CodexAuthState {
    pub fn has_usable_access_token(&self) -> bool {
        self.auth
            .tokens
            .as_ref()
            .and_then(|t| t.access_token.as_ref())
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn extracts_email_from_the_local_id_token() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"email":"person@example.com"}"#);
        let auth = CodexAuth {
            tokens: Some(CodexTokens {
                access_token: Some("access".to_string()),
                refresh_token: None,
                id_token: Some(format!("header.{payload}.signature")),
                account_id: None,
            }),
            last_refresh: None,
            openai_api_key: None,
        };

        assert_eq!(
            CodexAuthStore::account_email(&auth),
            Some("person@example.com".to_string())
        );
    }
}

pub struct CodexAuthStore;

impl CodexAuthStore {
    pub fn new() -> Self {
        Self
    }

    pub fn account_email(auth: &CodexAuth) -> Option<String> {
        let token = auth.tokens.as_ref()?.id_token.as_deref()?;
        let payload = token.split('.').nth(1)?;
        use base64::Engine;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        let email = claims.get("email")?.as_str()?.trim();
        if email.is_empty() || email.chars().any(char::is_control) {
            None
        } else {
            Some(email.to_string())
        }
    }

    /// Find auth.json candidates (CODEX_HOME env -> ~/.config/codex -> ~/.codex)
    pub fn auth_paths(&self) -> Vec<String> {
        if let Some(codex_home) = read_env_var("CODEX_HOME").filter(|v| !v.trim().is_empty()) {
            return vec![format!("{}/auth.json", codex_home.trim())];
        }
        vec![
            format!("{}/.config/codex/auth.json", Self::home()),
            format!("{}/.codex/auth.json", Self::home()),
        ]
    }

    pub fn load_auth_candidates(&self) -> Vec<CodexAuthState> {
        self.auth_paths()
            .into_iter()
            .filter_map(|path| self.load_auth(&path))
            .collect()
    }

    pub fn load_auth(&self, path: &str) -> Option<CodexAuthState> {
        let p = Path::new(path);
        if !p.exists() {
            return None;
        }
        let json = read_json_config(p).ok()??;
        let auth: CodexAuth = serde_json::from_value(json).ok()?;
        if !Self::has_token_like_auth(&auth) {
            return None;
        }
        Some(CodexAuthState {
            auth,
            source_path: Some(path.to_string()),
        })
    }

    pub fn has_token_like_auth(auth: &CodexAuth) -> bool {
        auth.tokens
            .as_ref()
            .and_then(|t| t.access_token.as_ref())
            .map(|t| !t.is_empty())
            .unwrap_or(false)
            || auth
                .openai_api_key
                .as_ref()
                .map(|k| !k.is_empty())
                .unwrap_or(false)
    }

    /// Simple JWT exp extraction (no verification, just base64 decode)
    pub fn access_token_expires_at(token: &str) -> Option<i64> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        use base64::Engine;
        let padded = match parts[1].len() % 4 {
            2 => format!("{}=", parts[1]),
            3 => format!("{}==", parts[1]),
            _ => parts[1].to_string(),
        };
        let decoded = base64::engine::general_purpose::URL_SAFE
            .decode(padded.as_bytes())
            .ok()?;
        let payload: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        payload
            .get("exp")
            .and_then(|v| v.as_f64())
            .map(|e| e as i64)
    }

    pub fn needs_refresh(&self, auth: &CodexAuth, now: i64) -> bool {
        if let Some(access_token) = auth.tokens.as_ref().and_then(|t| t.access_token.as_ref()) {
            if let Some(exp) = Self::access_token_expires_at(access_token) {
                return now >= exp - 300; // 5 min window
            }
        }
        if let Some(ref last_refresh) = auth.last_refresh {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last_refresh) {
                let age = now - dt.timestamp();
                return age > 8 * 24 * 3600;
            }
        }
        false
    }

    fn home() -> String {
        dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string())
    }
}
