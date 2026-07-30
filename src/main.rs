#![allow(dead_code)]

mod http;
mod models;
mod pricing;
mod providers;
mod tui;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::time::{Duration, Instant};
use std::{io, process::Command};
use tui::App;

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args_os().any(|argument| argument == "--version" || argument == "-V") {
        println!("openbroyach {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    tracing_subscriber::fmt::init();

    let mut terminal = ratatui::init();
    terminal.clear()?;

    let picker = Picker::from_query_stdio().unwrap_or_else(|error| {
        tracing::warn!(%error, "Terminal image detection failed; using half-block logos");
        let mut picker = Picker::from_fontsize((8, 16));
        picker.set_protocol_type(ProtocolType::Halfblocks);
        picker
    });

    let mut app = App::new();
    app.initialise(Some(&picker));

    let result = run_app(&mut terminal, &mut app).await;

    ratatui::restore();
    result
}

async fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    // Auto-refresh on start
    app.refresh_all().await;
    let mut next_refresh = Instant::now() + AUTO_REFRESH_INTERVAL;

    let mut selected_index: usize = 0;

    while !app.should_quit {
        let remaining = next_refresh.saturating_duration_since(Instant::now());
        app.refresh_status = format_refresh_countdown(remaining);
        terminal.draw(|frame| tui::widgets::draw(app, frame, selected_index))?;

        if event::poll(remaining.min(EVENT_POLL_INTERVAL))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('r') => {
                        app.refresh_all().await;
                        next_refresh = Instant::now() + AUTO_REFRESH_INTERVAL;
                    }
                    KeyCode::Char(shortcut @ '1'..='9') => {
                        if let Some((label, url)) = provider_link(app, selected_index, shortcut) {
                            if let Err(error) = open_url(&url) {
                                tracing::error!(%error, %url, "Failed to open provider link");
                                app.status_message = format!("Could not open {label}: {error}");
                            }
                        }
                    }
                    KeyCode::Up => {
                        selected_index = selected_index.saturating_sub(1);
                    }
                    KeyCode::Char('k') => {
                        selected_index = selected_index.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if selected_index + 1 < app.providers.len() {
                            selected_index += 1;
                        }
                    }
                    KeyCode::Char('j') => {
                        if selected_index + 1 < app.providers.len() {
                            selected_index += 1;
                        }
                    }
                    KeyCode::Char('K') => {
                        selected_index = app.move_provider_up(selected_index);
                    }
                    KeyCode::Char('J') => {
                        selected_index = app.move_provider_down(selected_index);
                    }
                    KeyCode::Home => {
                        selected_index = 0;
                    }
                    KeyCode::End => {
                        selected_index = app.providers.len().saturating_sub(1);
                    }
                    KeyCode::Left => {
                        app.collapse(selected_index);
                    }
                    KeyCode::Right => {
                        app.expand(selected_index);
                    }
                    KeyCode::Enter | KeyCode::Char(' ') if !app.providers.is_empty() => {
                        app.toggle_expand(selected_index);
                    }
                    _ => {}
                }
            }
        }

        if Instant::now() >= next_refresh && !app.should_quit {
            app.refresh_all().await;
            next_refresh = Instant::now() + AUTO_REFRESH_INTERVAL;
        }
    }

    Ok(())
}

fn format_refresh_countdown(remaining: Duration) -> String {
    let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
    format!("Next refresh in {}m {:02}s", seconds / 60, seconds % 60)
}

fn provider_link(app: &App, provider_index: usize, shortcut: char) -> Option<(String, String)> {
    let link_index = shortcut.to_digit(10)? as usize - 1;
    let link = app.providers.get(provider_index)?.links.get(link_index)?;
    Some((link.label.clone(), link.url.clone()))
}

fn open_url(url: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_auto_refresh_countdown() {
        assert_eq!(
            format_refresh_countdown(Duration::from_secs(300)),
            "Next refresh in 5m 00s"
        );
        assert_eq!(
            format_refresh_countdown(Duration::from_millis(60_001)),
            "Next refresh in 1m 01s"
        );
    }

    #[test]
    fn resolves_numbered_links_for_the_selected_provider() {
        let mut app = App::new();
        app.providers.push(tui::ProviderEntry {
            id: "codex".to_string(),
            display_name: "Codex".to_string(),
            account_label: None,
            links: vec![
                models::ProviderLink {
                    label: "Status".to_string(),
                    url: "https://status.example.com".to_string(),
                },
                models::ProviderLink {
                    label: "Dashboard".to_string(),
                    url: "https://example.com/dashboard".to_string(),
                },
            ],
            logo: None,
            snapshot: None,
            error: None,
            expanded: true,
        });

        assert_eq!(
            provider_link(&app, 0, '2'),
            Some((
                "Dashboard".to_string(),
                "https://example.com/dashboard".to_string()
            ))
        );
        assert_eq!(provider_link(&app, 0, '3'), None);
    }
}
