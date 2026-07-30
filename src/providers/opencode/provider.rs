use super::auth::*;
use super::mapper::*;
use super::scanner::*;
use super::windows::OpenCodeGoWindowMath;
use crate::models::*;
use crate::providers::{append_no_data_if_needed, spend_tile_mapper, ErrorCategory};

pub struct OpenCodeProvider {
    pub provider: Provider,
    pub auth_store: OpenCodeAuthStore,
    pub scanner: OpenCodeUsageScanner,
    source_note: String,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self {
            provider: Provider::with_links(
                "opencode",
                "OpenCode",
                vec![("Dashboard", "https://opencode.ai/auth")],
            ),
            auth_store: OpenCodeAuthStore::new(),
            scanner: OpenCodeUsageScanner::new(),
            source_note: "From your OpenCode logs".to_string(),
        }
    }

    pub fn widget_descriptors(&self) -> Vec<WidgetDescriptor> {
        let mut descriptors = vec![
            WidgetDescriptor::bounded_dollars(
                "opencode.session",
                "opencode",
                "Session",
                OpenCodeUsageMapper::SESSION_CAP,
            ),
            WidgetDescriptor::bounded_dollars(
                "opencode.weekly",
                "opencode",
                "Weekly",
                OpenCodeUsageMapper::WEEKLY_CAP,
            ),
            WidgetDescriptor::bounded_dollars(
                "opencode.monthly",
                "opencode",
                "Monthly",
                OpenCodeUsageMapper::MONTHLY_CAP,
            ),
        ];
        descriptors.extend(WidgetDescriptor::spend_tiles("opencode"));
        descriptors
    }

    pub fn has_local_credentials(&self) -> bool {
        match self.auth_store.go_api_key() {
            Ok(Some(_)) => true,
            Err(_) => true, // unreadable auth.json still signals OpenCode footprint
            Ok(None) => self.scanner.has_hosted_usage(),
        }
    }

    pub async fn refresh(&self) -> ProviderSnapshot {
        let now = chrono::Utc::now();
        let now_ms = now.timestamp_millis();

        let has_go_key = match self.auth_store.go_api_key() {
            Ok(Some(_)) => true,
            Err(_) => false,
            Ok(None) => false,
        };

        let scan = match self.scanner.scan(now) {
            Ok(Some(s)) => s,
            Ok(None) => {
                if has_go_key {
                    let windows = OpenCodeGoWindowMath::compute(&[], None, now);
                    return ProviderSnapshot::make(
                        &self.provider,
                        Some("Go"),
                        OpenCodeUsageMapper::meter_lines(&windows),
                        now_ms,
                        None,
                        None,
                    );
                }
                return ProviderSnapshot::error(
                    &self.provider,
                    "OpenCode not detected. Log in with OpenCode Go or use OpenCode locally first.",
                    Some(ErrorCategory::NotLoggedIn.as_str()),
                );
            }
            Err(e) => {
                return ProviderSnapshot::error(
                    &self.provider,
                    &e.to_string(),
                    Some(ErrorCategory::CredentialAccess.as_str()),
                );
            }
        };

        let mut lines: Vec<MetricLine> = Vec::new();
        if let Some(ref windows) = scan.go_windows {
            lines.extend(OpenCodeUsageMapper::meter_lines(windows));
        }
        spend_tile_mapper::append_token_usage(
            &scan.log_scan.series,
            &mut lines,
            now,
            false,
            &scan.log_scan.unknown_models_by_day,
            scan.log_scan.model_usage.as_ref(),
            Some(&self.source_note),
        );
        append_no_data_if_needed(&mut lines);

        let plan = if scan.go_windows.is_some() {
            Some("Go")
        } else {
            None
        };
        ProviderSnapshot::make(&self.provider, plan, lines, now_ms, None, None)
    }
}
