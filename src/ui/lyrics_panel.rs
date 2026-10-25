use super::{
    library,
    theme::Palette,
    widgets::{block, buttons, buttons_height, clean},
};
use crate::{
    app::{App, LyricsPanelAction as Action, Target},
    lyrics::Provider,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render_tracks(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if !app.settings.preferences.lyrics.show_panel || area.width < 24 || area.height < 7 {
        library::tracks_table(frame, app, area, palette);
        return;
    }
    let parts = if area.width >= 85 {
        Layout::horizontal([
            Constraint::Min(40),
            Constraint::Length((area.width / 3).clamp(30, 65)),
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Min(3),
            Constraint::Length((area.height / 2).max(4)),
        ])
        .split(area)
    };
    library::tracks_table(frame, app, parts[0], palette);
    render(frame, app, parts[1], palette);
}

fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.lyrics.panel.area = area;
    let border = block(
        app.text("Song lyrics", "Текст песни").into(),
        palette,
        false,
    )
    .border_style(palette.text().fg(palette.separator));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let loading = app.lyrics.panel_loading();
    let query = app.lyrics.panel.query.clone();
    let selected = app.lyrics.panel.lyric().cloned();
    let current_provider = selected.as_ref().and_then(|lyric| {
        Provider::ALL
            .into_iter()
            .find(|p| p.name() == lyric.provider)
    });
    let enabled: Vec<_> = Provider::ALL
        .into_iter()
        .filter(|p| {
            app.settings.preferences.lyrics.allows(*p) && !app.lyrics.panel.excluded.contains(p)
        })
        .collect();
    let other = enabled.iter().any(|p| Some(*p) != current_provider);
    let controls = if inner.height < 7 {
        vec![(
            app.text("Open · F9", "Открыть · F9").into(),
            Target::LyricsPanel(Action::Open),
            query.is_some() && !loading,
        )]
    } else {
        vec![
            (
                if selected.is_some() {
                    app.text("Find elsewhere", "Найти в другом")
                } else {
                    app.text("Search again", "Найти снова")
                }
                .into(),
                Target::LyricsPanel(if selected.is_some() {
                    Action::Other
                } else {
                    Action::Retry
                }),
                query.is_some() && !loading && app.settings.preferences.lyrics.enabled && other,
            ),
            (
                app.text("Open · F9", "Открыть · F9").into(),
                Target::LyricsPanel(Action::Open),
                query.is_some() && !loading,
            ),
            (
                app.text("Settings", "Настройки").into(),
                Target::LyricsPanel(Action::Settings),
                true,
            ),
            ("×".into(), Target::LyricsPanel(Action::Close), true),
        ]
    };
    let footer_height = buttons_height(app, inner.width, &controls).min(inner.height);
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height,
    );
    let available = inner.height.saturating_sub(footer_height);
    let source = selected
        .as_ref()
        .map_or(app.text("Song lyrics", "Текст песни"), |lyric| {
            lyric.provider.as_str()
        });
    if available > 0 {
        frame.render_widget(
            Paragraph::new(clean(source)).style(palette.text().fg(palette.accent)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
    let title = selected
        .as_ref()
        .map(|lyric| format!("{} — {}", lyric.artist, lyric.title))
        .or_else(|| {
            query
                .as_ref()
                .map(|query| format!("{} — {}", query.artist, query.title))
        })
        .unwrap_or_default();
    let heading = Paragraph::new(clean(&title))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: false });
    let heading_height = heading
        .line_count(inner.width)
        .min(2)
        .min(available.saturating_sub(1) as usize) as u16;
    frame.render_widget(
        heading,
        Rect::new(inner.x, inner.y + 1, inner.width, heading_height),
    );
    let body = Rect::new(
        inner.x,
        inner.y + 1 + heading_height,
        inner.width,
        available.saturating_sub(1 + heading_height),
    );
    app.lyrics.panel.page_size = body.height as usize;
    app.lyrics.panel.scroll_max = 0;
    if body.height > 0 {
        if let Some(lyric) = selected {
            let position = app.position_ms();
            let lines = lyric.lines();
            let active = lines
                .iter()
                .enumerate()
                .filter_map(|(i, (time, _))| time.filter(|t| *t <= position).map(|_| i))
                .last();
            let rendered: Vec<_> = lines
                .iter()
                .enumerate()
                .map(|(i, (_, text))| {
                    Line::styled(
                        clean(text),
                        if Some(i) == active {
                            palette.text().fg(palette.accent).bg(palette.selection)
                        } else {
                            palette.text()
                        },
                    )
                })
                .collect();
            let paragraph = Paragraph::new(rendered.clone()).wrap(Wrap { trim: false });
            app.lyrics.panel.scroll_max = paragraph
                .line_count(body.width)
                .saturating_sub(body.height as usize);
            if app.lyrics.panel.follow
                && let Some(active) = active
            {
                app.lyrics.panel.offset = Paragraph::new(rendered[..active].to_vec())
                    .wrap(Wrap { trim: false })
                    .line_count(body.width)
                    .saturating_sub(body.height as usize / 2);
            }
            app.lyrics.panel.offset = app.lyrics.panel.offset.min(app.lyrics.panel.scroll_max);
            if lyric.instrumental {
                frame.render_widget(
                    Paragraph::new(app.text("Instrumental track", "Инструментальная композиция"))
                        .style(palette.text()),
                    body,
                );
            } else {
                frame.render_widget(
                    paragraph.scroll((app.lyrics.panel.offset.min(u16::MAX as usize) as u16, 0)),
                    body,
                );
            }
        } else {
            let message = if query.is_none() {
                app.text(
                    "Start playing a song.",
                    "Начните воспроизведение композиции.",
                )
                .into()
            } else if loading {
                app.text("Looking for lyrics…", "Поиск текста…").into()
            } else if !app.settings.preferences.lyrics.enabled {
                app.text(
                    "Online search is off. Enable it in Settings → Song lyrics.",
                    "Поиск в интернете выключен. Включите его в настройках текстов песен.",
                )
                .into()
            } else if app.lyrics.network_paused {
                app.text("Cannot reach lyric sources. Automatic search is paused. Press Search again when connected.", "Нет связи с источниками. Автопоиск приостановлен. После подключения нажмите Найти снова.").into()
            } else if !app.lyrics.panel.results.is_empty() {
                app.text(
                    "Choose the matching song in the full view.",
                    "Выберите подходящую запись в полном окне.",
                )
                .into()
            } else if enabled.is_empty() {
                app.text(
                    "No sources left. Choose sources in Settings.",
                    "Доступных источников нет. Выберите их в настройках.",
                )
                .into()
            } else {
                app.lyrics.panel.error.clone().unwrap_or_else(|| {
                    app.text(
                        "No lyrics found. Open the full view to refine the search.",
                        "Текст не найден. В полном окне можно уточнить запрос.",
                    )
                    .into()
                })
            };
            frame.render_widget(
                Paragraph::new(message)
                    .style(palette.text().fg(palette.muted))
                    .wrap(Wrap { trim: false }),
                body,
            );
        }
    }
    buttons(frame, app, footer, controls, palette);
}
