use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    claude_ai_oauth: OAuthCredential,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCredential {
    access_token: String,
    expires_at: Option<i64>,
    subscription_type: Option<String>,
}

pub struct ClaudeCredential {
    pub access_token: String,
    pub expires_at: Option<i64>,
    pub subscription_type: Option<String>,
}

pub struct ClaudeAuthStore;

impl ClaudeAuthStore {
    fn config_dir() -> Option<PathBuf> {
        std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))
    }

    pub fn has_credentials(&self) -> bool {
        Self::config_dir().is_some_and(|dir| dir.join(".credentials.json").is_file())
    }

    pub fn load(&self) -> Result<Option<ClaudeCredential>, ClaudeAuthError> {
        let Some(path) = Self::config_dir().map(|dir| dir.join(".credentials.json")) else {
            return Ok(None);
        };
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ClaudeAuthError::Unreadable),
        };
        let credentials: Credentials =
            serde_json::from_str(&text).map_err(|_| ClaudeAuthError::Invalid)?;
        let oauth = credentials.claude_ai_oauth;
        if oauth.access_token.trim().is_empty() {
            return Err(ClaudeAuthError::Invalid);
        }
        Ok(Some(ClaudeCredential {
            access_token: oauth.access_token,
            expires_at: oauth.expires_at,
            subscription_type: oauth.subscription_type,
        }))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeAuthError {
    #[error("Could not read Claude Code credentials")]
    Unreadable,
    #[error("Claude Code credentials are invalid")]
    Invalid,
}
