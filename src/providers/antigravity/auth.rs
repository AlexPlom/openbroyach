use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const REFRESH_BUFFER_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct AntigravityToken {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at_ms: Option<i64>,
}

impl AntigravityToken {
    pub fn access_is_usable(&self, now_ms: i64) -> bool {
        self.access_token.is_some()
            && self
                .expires_at_ms
                .map(|expiry| expiry > now_ms + REFRESH_BUFFER_SECONDS * 1000)
                .unwrap_or(true)
    }
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityAuthError {
    #[error("Couldn't read Antigravity credentials from the system credential store. Unlock it or sign in to Antigravity again.")]
    CredentialStoreUnreadable,
    #[error(
        "Antigravity credentials are invalid. Open Antigravity or run `agy` to sign in again."
    )]
    InvalidCredentialData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedToken {
    access_token: String,
    expires_at_ms: i64,
    credential_fingerprint: String,
}

pub struct AntigravityAuthStore {
    cache_path: PathBuf,
}

impl AntigravityAuthStore {
    pub fn new() -> Self {
        let root = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        Self {
            cache_path: root
                .join("OpenBroyach")
                .join("antigravity")
                .join("auth.json"),
        }
    }

    #[cfg(test)]
    fn with_cache_path(cache_path: PathBuf) -> Self {
        Self { cache_path }
    }

    pub fn load_os_token(&self) -> Result<Option<AntigravityToken>, AntigravityAuthError> {
        let Some(raw) = read_os_credential()? else {
            return Ok(None);
        };
        Self::extract_token(&raw)
            .map(Some)
            .ok_or(AntigravityAuthError::InvalidCredentialData)
    }

    pub fn load_cached_token(&self, source: &AntigravityToken, now_ms: i64) -> Option<String> {
        let Some(expected) = fingerprint(source.refresh_token.as_deref()) else {
            self.discard_cached_token();
            return None;
        };
        let text = match std::fs::read_to_string(&self.cache_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                tracing::warn!("Antigravity refreshed-token cache read failed: {error}");
                return None;
            }
        };
        let cached: CachedToken = match serde_json::from_str(&text) {
            Ok(cached) => cached,
            Err(_) => {
                tracing::warn!("Antigravity refreshed-token cache is malformed; discarding it");
                self.discard_cached_token();
                return None;
            }
        };
        let token = trimmed(&cached.access_token);
        if cached.credential_fingerprint != expected
            || cached.expires_at_ms <= now_ms + REFRESH_BUFFER_SECONDS * 1000
            || token.is_none()
        {
            self.discard_cached_token();
            return None;
        }
        token
    }

    pub fn cache_token(
        &self,
        access_token: &str,
        expires_in_seconds: f64,
        source_refresh_token: &str,
        now_ms: i64,
    ) {
        let (Some(access_token), Some(credential_fingerprint)) = (
            trimmed(access_token),
            fingerprint(Some(source_refresh_token)),
        ) else {
            return;
        };
        let cached = CachedToken {
            access_token,
            expires_at_ms: now_ms.saturating_add((expires_in_seconds * 1000.0) as i64),
            credential_fingerprint,
        };
        let Some(parent) = self.cache_path.parent() else {
            return;
        };
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!("Failed to create Antigravity token cache directory: {error}");
            return;
        }
        if let Ok(data) = serde_json::to_vec(&cached) {
            if let Err(error) = std::fs::write(&self.cache_path, data) {
                tracing::warn!("Failed to cache refreshed Antigravity token: {error}");
            }
        }
    }

    pub fn discard_cached_token(&self) {
        match std::fs::remove_file(&self.cache_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!("Failed to remove stale Antigravity token cache: {error}");
            }
        }
    }

    pub fn extract_token(raw: &str) -> Option<AntigravityToken> {
        let normalized = raw.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        let decoded;
        let text = if let Some(encoded) = normalized.strip_prefix("go-keyring-base64:") {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .ok()?;
            decoded = String::from_utf8(bytes).ok()?;
            decoded.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        } else {
            normalized
        };
        if text.is_empty() {
            return None;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
            return match json {
                serde_json::Value::Object(object) => token_from_object(&object),
                serde_json::Value::String(value) => trimmed(&value).map(access_only),
                _ => None,
            };
        }
        if text.starts_with('{') || text.starts_with('[') {
            return None;
        }
        if let Some(token) = text.strip_prefix("Bearer ") {
            return trimmed(token).map(access_only);
        }
        trimmed(text).map(access_only)
    }
}

fn access_only(access_token: String) -> AntigravityToken {
    AntigravityToken {
        access_token: Some(access_token),
        refresh_token: None,
        expires_at_ms: None,
    }
}

fn token_from_object(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<AntigravityToken> {
    let source = object
        .get("token")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(object);
    let access_token = first_string(
        source,
        &[
            "access_token",
            "accessToken",
            "token",
            "id_token",
            "idToken",
            "bearerToken",
            "auth_token",
            "authToken",
        ],
    );
    let refresh_token = first_string(source, &["refresh_token", "refreshToken"]);
    let expires_at_ms = first_string(source, &["expiry", "expires_at", "expiresAt"])
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
        .map(|date| date.timestamp_millis());

    if access_token.is_some() || refresh_token.is_some() {
        return Some(AntigravityToken {
            access_token,
            refresh_token,
            expires_at_ms,
        });
    }
    for key in ["tokens", "oauth", "oauth2", "credentials", "auth"] {
        if let Some(nested) = object.get(key).and_then(serde_json::Value::as_object) {
            if let Some(token) = token_from_object(nested) {
                return Some(token);
            }
        }
    }
    None
}

fn first_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .and_then(trimmed)
    })
}

fn trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn fingerprint(refresh_token: Option<&str>) -> Option<String> {
    let refresh_token = refresh_token.and_then(trimmed)?;
    let digest = Sha256::digest(refresh_token.as_bytes());
    Some(base64::engine::general_purpose::STANDARD.encode(digest))
}

#[cfg(windows)]
fn read_os_credential() -> Result<Option<String>, AntigravityAuthError> {
    use std::ffi::c_void;
    use std::slice;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target: Vec<u16> = "gemini:antigravity\0".encode_utf16().collect();
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    let read = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if read == 0 {
        return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(None)
        } else {
            Err(AntigravityAuthError::CredentialStoreUnreadable)
        };
    }
    if credential.is_null() {
        return Err(AntigravityAuthError::CredentialStoreUnreadable);
    }

    let result = unsafe {
        let value = &*credential;
        let username = wide_string(value.UserName);
        let result = if username.as_deref() != Some("antigravity") {
            Err(AntigravityAuthError::InvalidCredentialData)
        } else if value.CredentialBlob.is_null() || value.CredentialBlobSize == 0 {
            Err(AntigravityAuthError::InvalidCredentialData)
        } else {
            let bytes =
                slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize);
            String::from_utf8(bytes.to_vec())
                .map(Some)
                .map_err(|_| AntigravityAuthError::InvalidCredentialData)
        };
        CredFree(credential.cast::<c_void>());
        result
    };
    result
}

#[cfg(windows)]
unsafe fn wide_string(pointer: *const u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut length = 0;
    while *pointer.add(length) != 0 {
        length += 1;
    }
    String::from_utf16(std::slice::from_raw_parts(pointer, length)).ok()
}

#[cfg(target_os = "macos")]
fn read_os_credential() -> Result<Option<String>, AntigravityAuthError> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-a",
            "antigravity",
            "-s",
            "gemini",
            "-w",
        ])
        .output()
        .map_err(|_| AntigravityAuthError::CredentialStoreUnreadable)?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout)
            .map_err(|_| AntigravityAuthError::InvalidCredentialData)?;
        return trimmed(&value)
            .map(Some)
            .ok_or(AntigravityAuthError::InvalidCredentialData);
    }
    if output.status.code() == Some(44) {
        Ok(None)
    } else {
        Err(AntigravityAuthError::CredentialStoreUnreadable)
    }
}

#[cfg(target_os = "linux")]
fn read_os_credential() -> Result<Option<String>, AntigravityAuthError> {
    let entry = keyring::Entry::new("gemini", "antigravity")
        .map_err(|_| AntigravityAuthError::CredentialStoreUnreadable)?;
    let outcome = match entry.get_password() {
        Ok(value) => LinuxCredentialOutcome::Found(value),
        Err(keyring::Error::NoEntry) => LinuxCredentialOutcome::Missing,
        Err(_) => LinuxCredentialOutcome::Unreadable,
    };
    resolve_linux_credential(outcome)
}

#[cfg(any(target_os = "linux", test))]
enum LinuxCredentialOutcome {
    Found(String),
    Missing,
    Unreadable,
}

#[cfg(any(target_os = "linux", test))]
fn resolve_linux_credential(
    outcome: LinuxCredentialOutcome,
) -> Result<Option<String>, AntigravityAuthError> {
    match outcome {
        LinuxCredentialOutcome::Found(value) => trimmed(&value)
            .map(Some)
            .ok_or(AntigravityAuthError::InvalidCredentialData),
        LinuxCredentialOutcome::Missing => Ok(None),
        LinuxCredentialOutcome::Unreadable => Err(AntigravityAuthError::CredentialStoreUnreadable),
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn read_os_credential() -> Result<Option<String>, AntigravityAuthError> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_credential_shapes_and_nested_keys() {
        let json = r#"{"token":{"access_token":"ya29.test","refresh_token":"1//refresh","expiry":"2099-01-01T00:00:00Z"}}"#;
        let wrapped = format!(
            "go-keyring-base64:{}",
            base64::engine::general_purpose::STANDARD.encode(json)
        );
        let parsed = AntigravityAuthStore::extract_token(&wrapped).unwrap();
        assert_eq!(parsed.access_token.as_deref(), Some("ya29.test"));
        assert_eq!(parsed.refresh_token.as_deref(), Some("1//refresh"));
        assert!(parsed.expires_at_ms.is_some());

        assert_eq!(
            AntigravityAuthStore::extract_token(r#""quoted""#)
                .unwrap()
                .access_token
                .as_deref(),
            Some("quoted")
        );
        assert_eq!(
            AntigravityAuthStore::extract_token("Bearer bearer")
                .unwrap()
                .access_token
                .as_deref(),
            Some("bearer")
        );
        assert_eq!(
            AntigravityAuthStore::extract_token("raw")
                .unwrap()
                .access_token
                .as_deref(),
            Some("raw")
        );
        let nested = AntigravityAuthStore::extract_token(
            r#"{"credentials":{"oauth2":{"refreshToken":"refresh-only"}}}"#,
        )
        .unwrap();
        assert_eq!(nested.refresh_token.as_deref(), Some("refresh-only"));
    }

    #[test]
    fn rejects_empty_invalid_base64_and_malformed_structured_material() {
        for raw in ["", "go-keyring-base64:not-base64", "{broken", "[broken"] {
            assert!(AntigravityAuthStore::extract_token(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn linux_credential_outcomes_keep_missing_unreadable_and_invalid_distinct() {
        assert_eq!(
            resolve_linux_credential(LinuxCredentialOutcome::Found(" token ".into())).unwrap(),
            Some("token".into())
        );
        assert_eq!(
            resolve_linux_credential(LinuxCredentialOutcome::Missing).unwrap(),
            None
        );
        assert_eq!(
            resolve_linux_credential(LinuxCredentialOutcome::Unreadable),
            Err(AntigravityAuthError::CredentialStoreUnreadable)
        );
        assert_eq!(
            resolve_linux_credential(LinuxCredentialOutcome::Found("  ".into())),
            Err(AntigravityAuthError::InvalidCredentialData)
        );
    }

    #[test]
    fn access_expiry_requires_more_than_sixty_seconds() {
        let token = AntigravityToken {
            access_token: Some("access".into()),
            refresh_token: None,
            expires_at_ms: Some(1_061_000),
        };
        assert!(token.access_is_usable(1_000_000));
        assert!(!token.access_is_usable(1_001_000));
    }

    #[test]
    fn cache_is_bound_to_refresh_fingerprint_and_expiry() {
        let path = std::env::temp_dir().join(format!(
            "openbroyach-antigravity-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = AntigravityAuthStore::with_cache_path(path.clone());
        let source = AntigravityToken {
            access_token: None,
            refresh_token: Some("refresh-a".into()),
            expires_at_ms: None,
        };
        store.cache_token("cached", 120.0, "refresh-a", 1_000_000);
        assert_eq!(
            store.load_cached_token(&source, 1_000_000).as_deref(),
            Some("cached")
        );
        let mismatch = AntigravityToken {
            refresh_token: Some("refresh-b".into()),
            ..source.clone()
        };
        assert!(store.load_cached_token(&mismatch, 1_000_000).is_none());
        assert!(!path.exists());

        store.cache_token("cached", 30.0, "refresh-a", 1_000_000);
        assert!(store.load_cached_token(&source, 1_000_000).is_none());
        assert!(!path.exists());

        std::fs::write(&path, "not json").unwrap();
        assert!(store.load_cached_token(&source, 1_000_000).is_none());
        assert!(!path.exists());

        store.cache_token("cached", 120.0, "refresh-a", 1_000_000);
        let no_refresh = AntigravityToken {
            access_token: Some("access".into()),
            refresh_token: None,
            expires_at_ms: None,
        };
        assert!(store.load_cached_token(&no_refresh, 1_000_000).is_none());
        assert!(!path.exists());
    }
}
