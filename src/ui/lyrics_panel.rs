use super::{
    theme::Palette,
    widgets::{buttons, buttons_height, clean, visible_offset},
};
use crate::{
    app::{App, Hit, LyricsPanelAction as Action, Target},
    lyrics::Provider,
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &mut App, inner: Rect, palette: Palette) {
    app.lyrics.panel.area = inner;
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let loading = app.lyrics.panel_loading();
    let query = app.lyrics.panel.query.clone();
    let selected = app.lyrics.panel.lyric().cloned();
    let choosing = !loading && selected.is_none() && !app.lyrics.panel.results.is_empty();
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
    let mut controls = if inner.height < 7 && choosing {
        vec![(
            app.text("Choose", "Выбрать").into(),
            Target::LyricsPanel(Action::Choose(app.lyrics.panel.cursor)),
            true,
        )]
    } else if inner.height < 7 {
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
    if inner.height >= 4 {
        controls.insert(0, (
            app.text("Save lyrics", "Сохранить текст").into(),
            Target::LyricsPanel(Action::Save),
            selected.is_some() && !loading && !app.lyrics_storage_busy(),
        ));
        if app.lyrics.panel.saved {
            controls.insert(1, (
                app.text("Delete lyrics", "Удалить текст").into(),
                Target::LyricsPanel(Action::Remove),
                !loading && !app.lyrics_storage_busy(),
            ));
        }
    }
    if inner.height >= 7 {
        if choosing && app.lyrics.panel.selected.is_some() {
            controls.insert(
                1,
                (
                    app.text("Back to lyrics", "К тексту").into(),
                    Target::LyricsPanel(Action::Content),
                    true,
                ),
            );
        } else if selected.is_some() && app.lyrics.panel.results.len() > 1 {
            controls.insert(
                1,
                (
                    app.text("Variants", "Варианты").into(),
                    Target::LyricsPanel(Action::Variants),
                    true,
                ),
            );
        }
    }
    let footer_height = buttons_height(app, inner.width, &controls).min(inner.height);
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height,
    );
    let available = inner.height.saturating_sub(footer_height);
    let source = selected.as_ref().map_or(
        if choosing {
            app.text("Found songs", "Найденные варианты")
        } else {
            app.text("Song lyrics", "Текст песни")
        },
        |lyric| lyric.provider.as_str(),
    );
    if available > 0 {
        let source = if app.lyrics.panel.saved {
            format!("{source} · {}", app.text("Local copy available", "Есть сохраненная копия"))
        } else { source.to_owned() };
        frame.render_widget(
            Paragraph::new(clean(&source)).style(palette.text().fg(palette.accent)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
    let title = selected
        .as_ref()
        .map(|lyric| song_title(&lyric.artist, &lyric.title))
        .or_else(|| {
            query
                .as_ref()
                .map(|query| song_title(&query.artist, &query.title))
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
        app.view.hits.push(Hit {
            area: body,
            target: Target::LyricsPanel(Action::Focus),
            enabled: true,
        });
        if choosing {
            render_choices(frame, app, body, palette);
        } else if let Some(lyric) = selected {
            let position = app.position_ms();
            let lines = lyric.lines();
            let active = lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| line.time.filter(|t| *t <= position).map(|_| i))
                .last();
            let rendered: Vec<_> = lines
                .iter()
                .enumerate()
                .map(|(i, line)| render_lyric_line(line, Some(position), Some(i) == active, palette))
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

fn song_title(artist: &str, title: &str) -> String {
    if artist.trim().is_empty() {
        title.to_owned()
    } else if title.trim().is_empty() {
        artist.to_owned()
    } else {
        format!("{artist} - {title}")
    }
}

pub(super) fn render_lyric_line(line: &crate::lyrics::LyricLine, position: Option<u64>, active: bool, palette: Palette) -> Line<'static> {
    let style = if active { palette.text().fg(palette.accent).bg(palette.selection) } else { palette.text() };
    if active && let Some(word) = position.and_then(|position| line.word_at(position)) {
        Line::from(vec![
            Span::styled(clean(&line.text[..word.start]), style),
            Span::styled(clean(&line.text[word.clone()]), style.fg(palette.selection_text).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)),
            Span::styled(clean(&line.text[word.end..]), style),
        ])
    } else { Line::styled(clean(&line.text), style) }
}

fn render_choices(frame: &mut Frame, app: &mut App, body: Rect, palette: Palette) {
    let hint = u16::from(body.height >= 4);
    if hint > 0 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Choose: click or ↑↓ and Enter",
                "Выбор: мышь или ↑↓ и Enter",
            ))
            .style(palette.text().fg(palette.muted)),
            Rect::new(body.x, body.y, body.width, hint),
        );
    }
    let row_height = body.height.saturating_sub(hint).min(3).max(1);
    let capacity = (body.height.saturating_sub(hint) / row_height) as usize;
    let count = app.lyrics.panel.results.len();
    let cursor = app.lyrics.panel.cursor.min(count.saturating_sub(1));
    let offset = visible_offset(app.lyrics.panel.offset, cursor, capacity, count);
    app.lyrics.panel.cursor = cursor;
    app.lyrics.panel.offset = offset;
    app.lyrics.panel.page_size = capacity;
    app.lyrics.panel.scroll_max = count.saturating_sub(capacity);
    for (index, lyric) in app
        .lyrics
        .panel
        .results
        .iter()
        .enumerate()
        .skip(offset)
        .take(capacity)
    {
        let row = Rect::new(
            body.x,
            body.y + hint + ((index - offset) as u16 * row_height),
            body.width,
            row_height,
        );
        let selected = index == cursor;
        let style = if selected || app.hovered(row) {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        let title = format!("{}. {}", index + 1, song_title(&lyric.artist, &lyric.title));
        let duration = lyric
            .duration
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            .map(|seconds| format!(" · {}", duration_text((seconds * 1000.0) as u64)))
            .unwrap_or_default();
        let source = format!(
            "{}{duration}{}",
            lyric.provider,
            if lyric.instrumental {
                app.text(" · instrumental", " · без вокала")
            } else {
                ""
            }
        );
        frame.render_widget(Paragraph::new("").style(style), row);
        frame.render_widget(
            Paragraph::new(clean(&title))
                .style(style)
                .wrap(Wrap { trim: false }),
            Rect::new(row.x, row.y, row.width, row.height.saturating_sub(1).max(1)),
        );
        if row.height > 1 {
            frame.render_widget(
                Paragraph::new(clean(&source)).style(if selected {
                    style
                } else {
                    style.fg(palette.muted)
                }),
                Rect::new(row.x, row.bottom() - 1, row.width, 1),
            );
        }
        app.view.hits.push(Hit {
            area: row,
            target: Target::LyricsPanel(Action::Choose(index)),
            enabled: true,
        });
    }
}
