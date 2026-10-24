use crate::{
    app::{App, Hit, LyricsAction, LyricsView, Target},
    ui::{
        theme::Palette,
        widgets::{buttons, buttons_height, clean, file_modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};

fn action(label: &str, value: LyricsAction, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Lyrics(value), enabled)
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut LyricsView,
    area: Rect,
    palette: Palette,
) {
    let inner = file_modal(
        frame,
        app,
        area,
        app.text("Song lyrics", "Текст песни").into(),
        46,
        palette,
    );
    if inner.width < 24 || inner.height < 18 {
        return;
    }
    let controls = controls(app, view, app.lyrics_loading());
    let footer_height = buttons_height(app, inner.width, &controls).max(2);
    let footer = Rect::new(
        inner.x,
        inner.bottom() - footer_height,
        inner.width,
        footer_height,
    );
    if view.confirm_remove {
        frame.render_widget(
            Paragraph::new(app.text(
                "Remove the lyrics saved in Almavorn?",
                "Удалить текст, сохраненный в Almavorn?",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        buttons(
            frame,
            app,
            footer,
            vec![
                action(
                    app.text("Cancel [Enter]", "Отмена [Enter]"),
                    LyricsAction::Back,
                    true,
                ),
                action(
                    app.text("Remove [D]", "Удалить [D]"),
                    LyricsAction::ConfirmRemove,
                    true,
                ),
            ],
            palette,
        );
        return;
    }
    let title = app
        .lyrics
        .query
        .as_ref()
        .map(|q| format!("{} — {}", q.track.artist, q.track.title))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let error_height = if app.lyrics.error.is_some() { 3 } else { 0 };
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner
            .height
            .saturating_sub(3 + footer_height + error_height),
    );
    let loading = app.lyrics_loading();
    if loading {
        frame.render_widget(
            Paragraph::new(app.text("Looking for lyrics…", "Поиск текста песни…"))
                .style(palette.text()),
            body,
        );
    } else if view.content
        && let Some(lyric) = app.lyrics.results.get(view.selected)
    {
        let caption = format!(
            "{} · {}{}",
            lyric.provider,
            if app.lyrics.saved {
                app.text("Saved", "Сохранено")
            } else {
                app.text("Search result", "Результат поиска")
            },
            if !lyric.synced.is_empty() {
                app.text(" · synchronized", " · по времени песни")
            } else {
                ""
            }
        );
        frame.render_widget(
            Paragraph::new(caption).style(palette.text().fg(palette.muted)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        let position = app
            .lyrics
            .query
            .as_ref()
            .filter(|q| {
                app.playback
                    .current
                    .as_ref()
                    .is_some_and(|t| t.id == q.track.id)
            })
            .map(|_| app.position_ms());
        let lines = lyric.lines();
        let active = position.and_then(|position| {
            lines
                .iter()
                .enumerate()
                .filter_map(|(i, (time, _))| time.filter(|t| *t <= position).map(|_| i))
                .last()
        });
        let rendered = lines
            .iter()
            .enumerate()
            .map(|(i, (_, line))| {
                Line::styled(
                    clean(line),
                    if Some(i) == active {
                        palette.text().fg(palette.accent).bg(palette.selection)
                    } else {
                        palette.text()
                    },
                )
            })
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(rendered.clone()).wrap(Wrap { trim: false });
        let count = paragraph.line_count(body.width.max(1));
        app.view.dialog_scroll_max = count.saturating_sub(body.height as usize);
        if view.follow
            && let Some(active) = active
        {
            let above = Paragraph::new(rendered[..active].to_vec())
                .wrap(Wrap { trim: false })
                .line_count(body.width.max(1));
            view.offset = above
                .saturating_sub(body.height as usize / 2)
                .min(app.view.dialog_scroll_max);
        }
        view.offset = view.offset.min(app.view.dialog_scroll_max);
        if lyric.instrumental {
            frame.render_widget(
                Paragraph::new(app.text("Instrumental track", "Инструментальная композиция"))
                    .style(palette.text()),
                body,
            );
        } else {
            frame.render_widget(
                paragraph.scroll((view.offset.min(u16::MAX as usize) as u16, 0)),
                body,
            );
        }
    } else if app.lyrics.results.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text(
                "No lyrics found. Try changing the artist or title.",
                "Текст не найден. Попробуйте уточнить исполнителя или название.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            body,
        );
    } else {
        frame.render_widget(
            Paragraph::new(app.text(
                "Choose the matching recording. Enter: open.",
                "Выберите подходящую запись. Enter: открыть.",
            ))
            .style(palette.text().fg(palette.muted)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        view.offset = visible_offset(
            view.offset,
            view.selected,
            body.height as usize,
            app.lyrics.results.len(),
        );
        for (row, index) in (view.offset..app.lyrics.results.len())
            .take(body.height as usize)
            .enumerate()
        {
            let lyric = &app.lyrics.results[index];
            let area = Rect::new(body.x, body.y + row as u16, body.width, 1);
            frame.render_widget(
                Paragraph::new(clean(&format!(
                    "{} — {} · {}",
                    lyric.artist, lyric.title, lyric.provider
                )))
                .style(if index == view.selected {
                    palette
                        .text()
                        .bg(palette.selection)
                        .fg(palette.selection_text)
                } else {
                    palette.text()
                }),
                area,
            );
            app.view.hits.push(Hit {
                area,
                target: Target::Lyrics(LyricsAction::Row(index)),
                enabled: true,
            });
        }
    }
    if let Some(error) = &app.lyrics.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    buttons(frame, app, footer, controls, palette);
}

fn controls(app: &App, view: &LyricsView, loading: bool) -> Vec<(String, Target, bool)> {
    let selected = !loading && view.selected < app.lyrics.results.len();
    vec![
        action(
            app.text("Search [F]", "Уточнить [F]"),
            LyricsAction::Search,
            true,
        ),
        action(
            app.text("Refresh [R]", "Искать снова [R]"),
            LyricsAction::Refresh,
            !loading,
        ),
        action(
            app.text("Save [S]", "Сохранить [S]"),
            LyricsAction::Save,
            selected,
        ),
        action(
            app.text("Edit [E]", "Изменить [E]"),
            LyricsAction::Edit,
            selected,
        ),
        action(
            app.text("Export [X]", "В файл [X]"),
            LyricsAction::Export,
            selected,
        ),
        action(
            if view.follow {
                app.text("[x] Follow [L]", "[x] Следить [L]")
            } else {
                app.text("[ ] Follow [L]", "[ ] Следить [L]")
            },
            LyricsAction::Follow,
            selected,
        ),
        action(
            app.text("Remove saved", "Удалить сохраненное"),
            LyricsAction::Remove,
            app.lyrics.saved,
        ),
        action(app.text("Back", "Назад"), LyricsAction::Back, true),
    ]
}
