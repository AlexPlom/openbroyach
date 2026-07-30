use crate::http::HttpClient;
use crate::models::{ProviderLink, ProviderSnapshot};
use crate::pricing::ModelPricing;
use crate::providers::antigravity::AntigravityProvider;
use crate::providers::codex::provider::CodexProvider;
use crate::providers::opencode::provider::OpenCodeProvider;
use crate::tui::logos::provider_logo;
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub enum AppScreen {
    Dashboard,
}

pub struct App {
    pub providers: Vec<ProviderEntry>,
    pub screen: AppScreen,
    pub should_quit: bool,
    pub now: chrono::DateTime<chrono::Utc>,
    pub refreshing: bool,
    pub refresh_status: String,
    pub status_message: String,
    provider_order_path: Option<PathBuf>,
}

pub struct ProviderEntry {
    pub id: String,
    pub display_name: String,
    pub account_label: Option<String>,
    pub links: Vec<ProviderLink>,
    pub logo: Option<StatefulProtocol>,
    pub snapshot: Option<ProviderSnapshot>,
    pub error: Option<String>,
    pub expanded: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            screen: AppScreen::Dashboard,
            should_quit: false,
            now: chrono::Utc::now(),
            refreshing: false,
            refresh_status: "Ready".to_string(),
            status_message: String::new(),
            provider_order_path: dirs::config_dir()
                .map(|root| root.join("OpenBroyach").join("provider-order.json")),
        }
    }

    pub fn initialise(&mut self, logo_picker: Option<&Picker>) {
        let opencode = OpenCodeProvider::new();
        if opencode.has_local_credentials() {
            let links = opencode
                .provider
                .visible_links()
                .into_iter()
                .cloned()
                .collect();
            self.providers.push(ProviderEntry {
                id: "opencode".to_string(),
                display_name: "OpenCode".to_string(),
                account_label: None,
                links,
                logo: logo_picker.and_then(|picker| provider_logo("opencode", picker)),
                snapshot: None,
                error: None,
                expanded: true,
            });
        }

        match ModelPricing::load() {
            Ok(pricing) => {
                let codex = CodexProvider::new(HttpClient::new(), pricing);
                if codex.has_local_credentials() {
                    let links = codex
                        .provider
                        .visible_links()
                        .into_iter()
                        .cloned()
                        .collect();
                    self.providers.insert(
                        0,
                        ProviderEntry {
                            id: "codex".to_string(),
                            display_name: "Codex".to_string(),
                            account_label: None,
                            links,
                            logo: logo_picker.and_then(|picker| provider_logo("codex", picker)),
                            snapshot: None,
                            error: None,
                            expanded: true,
                        },
                    );
                }
            }
            Err(error) => {
                self.status_message = format!("Pricing load failed: {error}");
            }
        }

        let antigravity = AntigravityProvider::new(HttpClient::new());
        if antigravity.has_local_credentials() {
            let insert_at = self
                .providers
                .iter()
                .position(|entry| entry.id == "opencode")
                .unwrap_or(self.providers.len());
            self.providers.insert(
                insert_at,
                ProviderEntry {
                    id: "antigravity".to_string(),
                    display_name: "Antigravity".to_string(),
                    account_label: None,
                    links: vec![],
                    logo: logo_picker.and_then(|picker| provider_logo("antigravity", picker)),
                    snapshot: None,
                    error: None,
                    expanded: true,
                },
            );
        }

        if self.providers.is_empty() {
            self.status_message = "No providers with local credentials found.".to_string();
        }
        self.apply_saved_provider_order();
    }

    pub async fn refresh_all(&mut self) {
        self.refreshing = true;
        self.refresh_status = "Refreshing...".to_string();
        self.now = chrono::Utc::now();

        let refresh_codex = self.providers.iter().any(|entry| entry.id == "codex");
        let refresh_antigravity = self.providers.iter().any(|entry| entry.id == "antigravity");
        let refresh_opencode = self.providers.iter().any(|entry| entry.id == "opencode");

        let codex_refresh = async move {
            if !refresh_codex {
                return None;
            }
            Some(match ModelPricing::load() {
                Ok(pricing) => Ok(CodexProvider::new(HttpClient::new(), pricing)
                    .refresh()
                    .await),
                Err(error) => Err(format!("Pricing load failed: {error}")),
            })
        };
        let opencode_refresh = async move {
            if refresh_opencode {
                Some(Ok(OpenCodeProvider::new().refresh().await))
            } else {
                None
            }
        };
        let antigravity_refresh = async move {
            if refresh_antigravity {
                Some(Ok(AntigravityProvider::new(HttpClient::new())
                    .refresh()
                    .await))
            } else {
                None
            }
        };
        let (codex_result, antigravity_result, opencode_result) =
            join_refreshes(codex_refresh, antigravity_refresh, opencode_refresh).await;

        if let Some(result) = codex_result {
            self.apply_refresh_result("codex", result);
        }
        if let Some(result) = antigravity_result {
            self.apply_refresh_result("antigravity", result);
        }
        if let Some(result) = opencode_result {
            self.apply_refresh_result("opencode", result);
        }

        self.now = chrono::Utc::now();
        self.refresh_status = format!("Refreshed at {}", self.now.format("%H:%M:%S"));
        self.refreshing = false;
    }

    fn apply_refresh_result(
        &mut self,
        provider_id: &str,
        incoming: Result<ProviderSnapshot, String>,
    ) {
        let Some(entry) = self
            .providers
            .iter_mut()
            .find(|entry| entry.id == provider_id)
        else {
            return;
        };
        let snapshot = match incoming {
            Ok(snapshot) => snapshot,
            Err(message) => {
                entry.error = Some(message);
                return;
            }
        };
        if let Some(message) = snapshot.error_message() {
            entry.error = Some(message.to_string());
            return;
        }
        entry.account_label = snapshot.account_label.clone();
        entry.snapshot = Some(snapshot);
        entry.error = None;
    }

    pub fn toggle_expand(&mut self, index: usize) {
        if let Some(entry) = self.providers.get_mut(index) {
            entry.expanded = !entry.expanded;
        }
    }

    pub fn expand(&mut self, index: usize) {
        if let Some(entry) = self.providers.get_mut(index) {
            entry.expanded = true;
        }
    }

    pub fn collapse(&mut self, index: usize) {
        if let Some(entry) = self.providers.get_mut(index) {
            entry.expanded = false;
        }
    }

    pub fn move_provider_up(&mut self, index: usize) -> usize {
        if index == 0 || index >= self.providers.len() {
            return index;
        }
        self.providers.swap(index, index - 1);
        self.persist_provider_order();
        index - 1
    }

    pub fn move_provider_down(&mut self, index: usize) -> usize {
        if index + 1 >= self.providers.len() {
            return index;
        }
        self.providers.swap(index, index + 1);
        self.persist_provider_order();
        index + 1
    }

    fn apply_saved_provider_order(&mut self) {
        let Some(path) = self.provider_order_path.as_ref() else {
            return;
        };
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(%error, "Could not read saved provider order");
                return;
            }
        };
        let order: ProviderOrder = match serde_json::from_str(&text) {
            Ok(order) => order,
            Err(error) => {
                tracing::warn!(%error, "Saved provider order is malformed");
                return;
            }
        };
        let ranks: HashMap<&str, usize> = order
            .provider_order
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        self.providers
            .sort_by_key(|entry| ranks.get(entry.id.as_str()).copied().unwrap_or(usize::MAX));
    }

    fn persist_provider_order(&self) {
        let Some(path) = self.provider_order_path.as_ref() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::error!(%error, "Could not create provider-order directory");
            return;
        }
        let order = ProviderOrder {
            provider_order: self
                .providers
                .iter()
                .map(|entry| entry.id.clone())
                .collect(),
        };
        match serde_json::to_vec_pretty(&order) {
            Ok(data) => {
                if let Err(error) = std::fs::write(path, data) {
                    tracing::error!(%error, "Could not save provider order");
                }
            }
            Err(error) => tracing::error!(%error, "Could not encode provider order"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOrder {
    provider_order: Vec<String>,
}

async fn join_refreshes<C, A, O>(
    codex: C,
    antigravity: A,
    opencode: O,
) -> (C::Output, A::Output, O::Output)
where
    C: std::future::Future,
    A: std::future::Future,
    O: std::future::Future,
{
    tokio::join!(codex, antigravity, opencode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MetricLine, Provider};
    use std::sync::Arc;

    fn entry() -> ProviderEntry {
        ProviderEntry {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            account_label: None,
            links: vec![],
            logo: None,
            snapshot: None,
            error: None,
            expanded: true,
        }
    }

    fn success(value: &str, refreshed_at: i64) -> ProviderSnapshot {
        ProviderSnapshot::make(
            &Provider::new("test", "Test"),
            None,
            vec![MetricLine::Text {
                label: "Usage".to_string(),
                value: value.to_string(),
                color_hex: None,
                subtitle: None,
            }],
            refreshed_at,
            None,
            None,
        )
    }

    #[test]
    fn refresh_failure_retains_last_good_snapshot_until_success() {
        let mut app = App::new();
        app.providers.push(entry());
        app.apply_refresh_result("test", Ok(success("old", 1)));

        let provider = Provider::new("test", "Test");
        app.apply_refresh_result(
            "test",
            Ok(ProviderSnapshot::error(
                &provider,
                "Network unavailable",
                Some("network"),
            )),
        );

        assert_eq!(app.providers[0].snapshot.as_ref().unwrap().refreshed_at, 1);
        assert_eq!(
            app.providers[0].error.as_deref(),
            Some("Network unavailable")
        );

        app.apply_refresh_result("test", Ok(success("new", 2)));
        assert_eq!(app.providers[0].snapshot.as_ref().unwrap().refreshed_at, 2);
        assert_eq!(app.providers[0].error, None);
    }

    #[tokio::test]
    async fn provider_refresh_futures_are_polled_concurrently() {
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let first_barrier = barrier.clone();
        let second_barrier = barrier.clone();
        let third_barrier = barrier.clone();
        let joined = join_refreshes(
            async move { first_barrier.wait().await },
            async move { second_barrier.wait().await },
            async move { third_barrier.wait().await },
        );

        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), joined)
                .await
                .is_ok()
        );
    }

    #[test]
    fn provider_reordering_persists_and_restores() {
        let root = std::env::temp_dir().join(format!("openbroyach-order-{}", uuid::Uuid::new_v4()));
        let path = root.join("provider-order.json");
        let mut app = App::new();
        app.provider_order_path = Some(path.clone());
        for id in ["codex", "antigravity", "opencode"] {
            let mut provider = entry();
            provider.id = id.to_string();
            app.providers.push(provider);
        }

        let selected = app.move_provider_down(0);

        assert_eq!(selected, 1);
        assert_eq!(
            app.providers
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["antigravity", "codex", "opencode"]
        );

        let mut restored = App::new();
        restored.provider_order_path = Some(path);
        for id in ["codex", "antigravity", "opencode"] {
            let mut provider = entry();
            provider.id = id.to_string();
            restored.providers.push(provider);
        }
        restored.apply_saved_provider_order();
        assert_eq!(
            restored
                .providers
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["antigravity", "codex", "opencode"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
