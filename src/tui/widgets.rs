use super::app::{App, ProviderEntry};
use crate::models::*;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use ratatui_image::{Resize, StatefulImage};

pub fn draw(app: &mut App, frame: &mut Frame, selected_index: usize) {
    let area = frame.area();
    if area.width < 32 || area.height < 7 {
        frame.render_widget(
            Paragraph::new("Terminal too small\nNeed at least 32 x 7")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_providers(frame, chunks[1], app, selected_index);
    draw_footer(frame, chunks[2]);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let border = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" OpenBroyach", Style::default().bold().fg(Color::Cyan)),
            Span::styled("  AI usage at a glance", Style::default().dim()),
        ]))
        .block(border.clone()),
        columns[0],
    );

    let status_style = if !app.status_message.is_empty() {
        Style::default().fg(Color::Red).bold()
    } else if app.refreshing {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::Gray)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "{} ",
                if app.status_message.is_empty() {
                    &app.refresh_status
                } else {
                    &app.status_message
                }
            ),
            status_style,
        )))
        .alignment(Alignment::Right)
        .block(border),
        columns[1],
    );
}

fn draw_providers(frame: &mut Frame, area: Rect, app: &mut App, selected_index: usize) {
    if app.providers.is_empty() {
        let message = if app.status_message.is_empty() {
            "No providers detected.\n\nSign in to Codex, Antigravity, or OpenCode, then restart."
                .to_string()
        } else {
            format!("No providers detected.\n\n{}", app.status_message)
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let mut lines = Vec::new();
    let mut ranges = Vec::with_capacity(app.providers.len());
    for (index, entry) in app.providers.iter().enumerate() {
        let start = lines.len();
        lines.extend(provider_lines(
            entry,
            index == selected_index,
            area.width.saturating_sub(2),
            app.now.timestamp_millis(),
        ));
        let end = lines.len();
        ranges.push((start, end));
        if index + 1 < app.providers.len() {
            lines.push(Line::from(""));
        }
    }

    let viewport_height = area.height as usize;
    let total_lines = lines.len();
    let (selected_start, selected_end) = ranges
        .get(selected_index)
        .copied()
        .unwrap_or((0, viewport_height));
    let selected_height = selected_end.saturating_sub(selected_start);
    let offset = if selected_height >= viewport_height {
        selected_start
    } else {
        selected_end.saturating_sub(viewport_height)
    }
    .min(total_lines.saturating_sub(viewport_height));

    let visible = lines
        .into_iter()
        .skip(offset)
        .take(viewport_height)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(visible)), area);

    for (index, (header_line, _)) in ranges.iter().copied().enumerate() {
        if header_line < offset || header_line >= offset + viewport_height {
            continue;
        }
        let Some(logo) = app.providers[index].logo.as_mut() else {
            continue;
        };
        frame.render_stateful_widget(
            StatefulImage::new().resize(Resize::Fit(None)),
            Rect::new(
                area.x.saturating_add(4),
                area.y.saturating_add((header_line - offset) as u16),
                2,
                1,
            ),
            logo,
        );
    }

    if total_lines > viewport_height {
        let mut state = ScrollbarState::new(total_lines).position(offset);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("┃"),
            area,
            &mut state,
        );
    }
}

fn provider_lines(
    entry: &ProviderEntry,
    selected: bool,
    width: u16,
    now_ms: i64,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut header = Vec::new();
    header.push(Span::styled(
        if selected { "▌ " } else { "  " },
        Style::default().fg(Color::Cyan).bold(),
    ));
    header.push(Span::styled(
        if entry.expanded { "▼ " } else { "▶ " },
        Style::default().fg(if selected { Color::Cyan } else { Color::Gray }),
    ));
    if entry.logo.is_some() {
        header.push(Span::raw("   "));
    } else {
        let (icon, icon_color) = provider_icon(&entry.id);
        header.push(Span::styled(
            format!("{} ", icon),
            Style::default().fg(icon_color).bold(),
        ));
    }
    header.push(Span::styled(
        entry.display_name.clone(),
        if selected {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::White).bold()
        },
    ));
    if let Some(account_label) = &entry.account_label {
        header.push(Span::styled(
            format!("  {}", account_label),
            Style::default().fg(Color::Gray),
        ));
    }

    if let Some(snapshot) = &entry.snapshot {
        if let Some(plan) = &snapshot.plan {
            header.push(Span::styled(
                format!("  [{}]", plan),
                Style::default().fg(Color::Green),
            ));
        }
        if entry.error.is_some() {
            header.push(Span::styled(
                "  STALE",
                Style::default().fg(Color::Yellow).bold(),
            ));
        } else if snapshot.lines.iter().any(MetricLine::is_error) {
            header.push(Span::styled(
                "  ERROR",
                Style::default().fg(Color::Red).bold(),
            ));
        } else if snapshot.warning.is_some() {
            header.push(Span::styled(
                "  WARNING",
                Style::default().fg(Color::Yellow).bold(),
            ));
        }
        if !entry.expanded {
            let summaries = snapshot
                .lines
                .iter()
                .filter_map(compact_metric)
                .take(2)
                .collect::<Vec<_>>();
            if !summaries.is_empty() {
                header.push(Span::styled(
                    format!("  {}", summaries.join(" · ")),
                    Style::default().fg(Color::Gray),
                ));
            }
        }
    }
    lines.push(Line::from(header));

    if !entry.expanded {
        return lines;
    }

    if let Some(snapshot) = &entry.snapshot {
        if let Some(error) = &entry.error {
            lines.push(Line::from(vec![
                Span::styled("    ! ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    format!("Refresh failed; showing last successful data: {error}"),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }
        if let Some(warning) = &snapshot.warning {
            lines.push(Line::from(vec![
                Span::styled("    ! ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(warning.clone(), Style::default().fg(Color::Yellow)),
            ]));
        }
        for metric in &snapshot.lines {
            lines.extend(metric_lines(metric, width, now_ms));
        }
        if !entry.links.is_empty() {
            let mut link_spans = vec![Span::raw("    ")];
            for (index, link) in entry.links.iter().take(9).enumerate() {
                link_spans.push(Span::styled(
                    format!(" [{}] {} ↗ ", index + 1, link.label),
                    Style::default().fg(Color::Cyan).bold(),
                ));
                link_spans.push(Span::raw(" "));
            }
            lines.push(Line::from(link_spans));
        }
    } else if let Some(error) = &entry.error {
        lines.push(Line::from(vec![
            Span::styled("    ERROR  ", Style::default().fg(Color::Red).bold()),
            Span::styled(error.clone(), Style::default().fg(Color::Red)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "    Waiting for first refresh...",
            Style::default().fg(Color::Gray).dim(),
        )));
    }

    lines
}

fn metric_lines(line: &MetricLine, width: u16, now_ms: i64) -> Vec<Line<'static>> {
    match line {
        MetricLine::Progress {
            label,
            used,
            limit,
            format,
            resets_at,
            color_hex,
            ..
        } => progress_lines(
            label,
            *used,
            *limit,
            format,
            *resets_at,
            color_hex.as_deref(),
            width,
            now_ms,
        ),
        MetricLine::Values {
            label,
            values,
            color_hex,
            expiries_at,
            unknown_models,
            ..
        } => {
            let style = optional_color(color_hex.as_deref()).unwrap_or(Color::White);
            let mut row = vec![
                Span::styled(
                    format!("    {:<21}", label),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(format_values(values), Style::default().fg(style).bold()),
            ];
            if !unknown_models.is_empty() {
                row.push(Span::styled(
                    format!("  ! {} unpriced", unknown_models.len()),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if let Some(expiry) = expiries_at.iter().min() {
                row.push(Span::styled(
                    format!("  expires {}", relative_time(*expiry, now_ms)),
                    Style::default().fg(Color::Gray),
                ));
            }
            vec![Line::from(row)]
        }
        MetricLine::Text {
            label,
            value,
            color_hex,
            subtitle,
        } => labeled_lines(label, value, color_hex.as_deref(), subtitle.as_deref()),
        MetricLine::Badge {
            label,
            text,
            color_hex,
            subtitle,
        } => labeled_lines(label, text, color_hex.as_deref(), subtitle.as_deref()),
        MetricLine::Chart {
            label,
            points,
            note,
        } => {
            let spark = sparkline(points, width.saturating_sub(25).clamp(8, 28) as usize);
            let mut result = vec![Line::from(vec![
                Span::styled(
                    format!("    {:<20}", label),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(spark, Style::default().fg(Color::Cyan).bold()),
            ])];
            if let Some(note) = note {
                result.push(Line::from(Span::styled(
                    format!("    {:<20}{}", "", note),
                    Style::default().fg(Color::DarkGray).dim(),
                )));
            }
            result
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn progress_lines(
    label: &str,
    used: f64,
    limit: f64,
    format: &ProgressFormat,
    resets_at: Option<i64>,
    color_hex: Option<&str>,
    width: u16,
    now_ms: i64,
) -> Vec<Line<'static>> {
    let ratio = if limit > 0.0 {
        (used / limit).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let color = optional_color(color_hex).unwrap_or_else(|| usage_color(ratio));
    let value = progress_value(used, limit, format);
    let reset = resets_at
        .map(|timestamp| format!("resets {}", relative_time(timestamp, now_ms)))
        .unwrap_or_default();
    let bar_width = match width {
        0..=44 => 12,
        45..=69 => 14,
        _ => 20,
    };
    let gauge = progress_bar(ratio, bar_width);

    if width >= 70 {
        vec![Line::from(vec![
            Span::styled(
                format!("    {:<20}", label),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(gauge, Style::default().fg(color)),
            Span::styled(
                format!("  {:>12}", value),
                Style::default().fg(Color::White).bold(),
            ),
            Span::styled(
                if reset.is_empty() {
                    String::new()
                } else {
                    format!("  {}", reset)
                },
                Style::default().fg(Color::Gray),
            ),
        ])]
    } else {
        vec![
            Line::from(vec![
                Span::styled(
                    format!("    {:<20}", label),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(value, Style::default().fg(Color::White).bold()),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(gauge, Style::default().fg(color)),
                Span::styled(
                    if reset.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", reset)
                    },
                    Style::default().fg(Color::Gray),
                ),
            ]),
        ]
    }
}

fn labeled_lines(
    label: &str,
    value: &str,
    color_hex: Option<&str>,
    subtitle: Option<&str>,
) -> Vec<Line<'static>> {
    let color = optional_color(color_hex).unwrap_or(Color::White);
    let mut result = vec![Line::from(vec![
        Span::styled(
            format!("    {:<20}", label),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(value.to_string(), Style::default().fg(color).bold()),
    ])];
    if let Some(subtitle) = subtitle {
        result.push(Line::from(Span::styled(
            format!("    {:<20}{}", "", subtitle),
            Style::default().fg(Color::DarkGray).dim(),
        )));
    }
    result
}

fn compact_metric(line: &MetricLine) -> Option<String> {
    match line {
        MetricLine::Progress {
            label,
            used,
            limit,
            format,
            ..
        } => Some(format!(
            "{} {}",
            label,
            progress_value(*used, *limit, format)
        )),
        MetricLine::Values { label, values, .. } if !values.is_empty() => {
            Some(format!("{} {}", label, format_values(values)))
        }
        _ => None,
    }
}

fn progress_value(used: f64, limit: f64, format: &ProgressFormat) -> String {
    match format {
        ProgressFormat::Percent => format!("{:.0}%", used),
        ProgressFormat::Dollars => format!("${:.2} / ${:.0}", used, limit),
        ProgressFormat::Count { suffix } => {
            format!("{:.0} / {:.0} {}", used, limit, suffix)
        }
    }
}

fn format_values(values: &[MetricValue]) -> String {
    values
        .iter()
        .map(|value| {
            let prefix = if value.estimated { "~" } else { "" };
            let number = match value.kind {
                MetricKind::Dollars => format!("${:.2}", value.number),
                MetricKind::Percent => format!("{:.0}%", value.number),
                MetricKind::Count => compact_number(value.number),
            };
            match &value.label {
                Some(label) => format!("{}{} {}", prefix, number, label),
                None => format!("{}{}", prefix, number),
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn compact_number(number: f64) -> String {
    if number >= 1_000_000_000.0 {
        format!("{:.1}B", number / 1_000_000_000.0)
    } else if number >= 1_000_000.0 {
        format!("{:.1}M", number / 1_000_000.0)
    } else if number >= 1_000.0 {
        format!("{:.1}K", number / 1_000.0)
    } else if number.fract() == 0.0 {
        format!("{:.0}", number)
    } else {
        format!("{:.1}", number)
    }
}

fn progress_bar(ratio: f64, width: usize) -> String {
    let filled = (ratio * width as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn usage_color(ratio: f64) -> Color {
    if ratio >= 0.9 {
        Color::Red
    } else if ratio >= 0.8 {
        Color::Yellow
    } else {
        Color::Cyan
    }
}

fn relative_time(timestamp_ms: i64, now_ms: i64) -> String {
    let delta_ms = timestamp_ms - now_ms;
    if delta_ms <= 0 {
        return "due".to_string();
    }
    let minutes = delta_ms / 60_000;
    if minutes < 1 {
        "in <1m".to_string()
    } else if minutes < 60 {
        format!("in {}m", minutes)
    } else if minutes < 24 * 60 {
        format!("in {}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("in {}d", minutes / (24 * 60))
    }
}

fn sparkline(points: &[MetricChartPoint], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if points.is_empty() {
        return "no activity".to_string();
    }
    let points = &points[points.len().saturating_sub(width)..];
    let max = points
        .iter()
        .map(|point| point.value.max(0.0))
        .fold(0.0_f64, f64::max);
    if max == 0.0 {
        return "▁".repeat(points.len());
    }
    points
        .iter()
        .map(|point| {
            let index = ((point.value.max(0.0) / max) * 7.0).round() as usize;
            BARS[index.min(7)]
        })
        .collect()
}

fn optional_color(hex: Option<&str>) -> Option<Color> {
    let hex = hex?.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    Some(Color::Rgb(
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

fn provider_icon(provider_id: &str) -> (&'static str, Color) {
    match provider_id {
        "antigravity" => ("A", Color::Blue),
        "codex" => ("✦", Color::Magenta),
        "opencode" => ("▣", Color::Cyan),
        _ => ("◇", Color::Gray),
    }
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let help = if area.width >= 82 {
        " j/k Select   J/K Move   ←→ Fold   Enter Toggle   1-9 Open   r Refresh   q Quit "
    } else if area.width >= 52 {
        " ↑↓ Select   J/K Move   Enter Toggle   r Refresh   q Quit "
    } else {
        " ↑↓ Move  Enter Toggle  r Refresh  q Quit "
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            help,
            Style::default().fg(Color::Gray),
        )]))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn app_with_provider(expanded: bool) -> App {
        let mut app = App::new();
        app.providers.push(ProviderEntry {
            id: "test".to_string(),
            display_name: "Test Provider".to_string(),
            account_label: Some("person@example.com".to_string()),
            links: vec![
                ProviderLink {
                    label: "Status".to_string(),
                    url: "https://status.example.com".to_string(),
                },
                ProviderLink {
                    label: "Dashboard".to_string(),
                    url: "https://example.com/dashboard".to_string(),
                },
            ],
            logo: None,
            snapshot: Some(ProviderSnapshot {
                provider_id: "test".to_string(),
                display_name: "Test Provider".to_string(),
                account_label: Some("person@example.com".to_string()),
                plan: Some("Paid Plan".to_string()),
                lines: vec![
                    MetricLine::Progress {
                        label: "Session".to_string(),
                        used: 42.0,
                        limit: 100.0,
                        format: ProgressFormat::Percent,
                        resets_at: Some(3_600_000),
                        period_duration_ms: None,
                        color_hex: None,
                    },
                    MetricLine::Text {
                        label: "Detail".to_string(),
                        value: "42".to_string(),
                        color_hex: None,
                        subtitle: Some("Local estimate".to_string()),
                    },
                    MetricLine::Values {
                        label: "Rate Limit Resets".to_string(),
                        values: vec![MetricValue::with_label(2.0, MetricKind::Count, "available")],
                        color_hex: None,
                        expiries_at: vec![],
                        unknown_models: vec![],
                        model_breakdown: None,
                    },
                ],
                refreshed_at: 0,
                usage_history: None,
                warning: None,
                error_category: None,
            }),
            error: None,
            expanded,
        });
        app.now = chrono::DateTime::from_timestamp_millis(0).unwrap();
        app
    }

    fn rendered_text_at_size(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(app, frame, 0)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn collapsed_provider_keeps_summary_but_hides_details() {
        let mut app = app_with_provider(false);

        let collapsed = rendered_text_at_size(&mut app, 100, 14);
        app.toggle_expand(0);
        let expanded = rendered_text_at_size(&mut app, 100, 14);

        assert!(collapsed.contains("Session 42%"));
        assert!(!collapsed.contains("Detail"));
        assert!(expanded.contains("Detail"));
        assert!(expanded.contains("Local estimate"));
    }

    #[test]
    fn provider_header_shows_the_account_label() {
        let mut app = app_with_provider(false);

        let rendered = rendered_text_at_size(&mut app, 100, 14);

        assert!(rendered.contains("Test Provider  person@example.com  [Paid Plan]"));
    }

    #[test]
    fn provider_icons_are_distinct_and_have_a_fallback() {
        assert_eq!(provider_icon("codex").0, "✦");
        assert_eq!(provider_icon("opencode").0, "▣");
        assert_eq!(provider_icon("antigravity").0, "A");
        assert_eq!(provider_icon("future-provider").0, "◇");
    }

    #[test]
    fn expanded_provider_shows_numbered_links() {
        let mut app = app_with_provider(true);

        let rendered = rendered_text_at_size(&mut app, 100, 16);

        assert!(rendered.contains("[1] Status"));
        assert!(rendered.contains("[2] Dashboard"));
    }

    #[test]
    fn retained_snapshot_shows_refresh_failure_and_stale_marker() {
        let mut app = app_with_provider(true);
        app.providers[0].error = Some("Network unavailable".to_string());

        let rendered = rendered_text_at_size(&mut app, 100, 16);

        assert!(rendered.contains("STALE"));
        assert!(
            rendered.contains("Refresh failed; showing last successful data: Network unavailable")
        );
        assert!(rendered.contains("Session"));
    }

    #[test]
    fn progress_metric_has_visual_gauge_and_reset() {
        let mut app = app_with_provider(true);

        let rendered = rendered_text_at_size(&mut app, 100, 14);

        assert!(rendered.contains("████████░░░░░░░░░░░░"));
        assert!(rendered.contains("resets in 1h 0m"));
    }

    #[test]
    fn long_metric_labels_keep_the_value_column_aligned() {
        let mut app = app_with_provider(true);

        let rendered = rendered_text_at_size(&mut app, 100, 14);

        assert!(rendered.contains("Rate Limit Resets    2 available"));
    }

    #[test]
    fn tiny_terminal_shows_size_message_without_panicking() {
        let mut app = app_with_provider(true);

        let rendered = rendered_text_at_size(&mut app, 20, 6);

        assert!(rendered.contains("Terminal too small"));
    }

    #[test]
    fn selected_provider_is_kept_in_view() {
        let mut app = app_with_provider(true);
        for index in 1..6 {
            app.providers.push(ProviderEntry {
                id: format!("test-{index}"),
                display_name: format!("Provider {index}"),
                account_label: None,
                links: vec![],
                logo: None,
                snapshot: None,
                error: None,
                expanded: false,
            });
        }

        let rendered = rendered_text_at_size(&mut app, 80, 10);
        let rendered_last = {
            let backend = TestBackend::new(80, 10);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| draw(&mut app, frame, 5)).unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };

        assert!(rendered.contains("Test Provider"));
        assert!(rendered_last.contains("Provider 5"));
    }
}
