use crate::providers::{expand_home, read_env_var};
use serde_json;
use std::path::Path;

pub struct OpenCodeAuthStore;

impl OpenCodeAuthStore {
    pub fn new() -> Self {
        Self
    }

    pub fn data_directory(&self) -> String {
        if let Some(override_path) = read_env_var("OPENCODE_DATA_DIR")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return expand_home(&override_path);
        }
        if let Some(xdg) = read_env_var("XDG_DATA_HOME")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return format!("{}/opencode", expand_home(&xdg));
        }
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        format!("{}/.local/share/opencode", home)
    }

    pub fn auth_file_path(&self) -> String {
        format!("{}/auth.json", self.data_directory())
    }

    /// Read the `opencode-go` key from auth.json
    pub fn go_api_key(&self) -> Result<Option<String>, OpenCodeAuthError> {
        let auth_path = self.auth_file_path();
        let path = Path::new(&auth_path);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| OpenCodeAuthError::CredentialsUnreadable(e.to_string()))?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| OpenCodeAuthError::CredentialsUnreadable(e.to_string()))?;
        let key = json
            .get("opencode-go")
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("key"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(key)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenCodeAuthError {
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("Credentials unreadable: {0}")]
    CredentialsUnreadable(String),
}
